use crate::ast::*;
use crate::generator::Generator;

fn generate_ls_helper(generator: &mut Generator, dir: &str, array_name: &str, sort_files: bool) -> String {
    let mut output = String::new();
    
    // Always declare the array first
    output.push_str(&generator.indent());
    output.push_str(&format!("my @{};\n", array_name));
    
    // For ls . command, search from current directory
    let search_dir = if dir == "." { "." } else { dir };
    
    // Check if this is a glob pattern (contains * or ?)
    let is_glob = search_dir.contains('*') || search_dir.contains('?');
    
    if is_glob {
        // For glob patterns, use Perl's glob function
        output.push_str(&generator.indent());
        output.push_str(&format!("@{} = glob('{}');\n", array_name, search_dir));
        
        if sort_files {
            output.push_str(&generator.indent());
            output.push_str(&format!("@{} = sort {{ $a cmp $b }} @{};\n", array_name, array_name));
        }
    } else {
        // For regular directories, use opendir/readdir
        if sort_files {
            // For sorting, we still need to collect files first
            output.push_str(&generator.indent());
            output.push_str(&format!("if (opendir(my $dh, '{}')) {{\n", search_dir));
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str("while (my $file = readdir($dh)) {\n");
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str("next if $file eq '.' || $file eq '..';\n");
            output.push_str(&generator.indent());
            output.push_str(&format!("push @{}, $file;\n", array_name));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
            output.push_str(&generator.indent());
            output.push_str("closedir($dh);\n");
            output.push_str(&generator.indent());
            output.push_str(&format!("@{} = sort {{ $a cmp $b }} @{};\n", array_name, array_name));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
        } else {
            // For non-sorting, collect to array instead of printing directly
            // This is needed for pipeline context where we need the array
            output.push_str(&generator.indent());
            output.push_str(&format!("if (opendir(my $dh, '{}')) {{\n", search_dir));
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str("while (my $file = readdir($dh)) {\n");
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str("next if $file eq '.' || $file eq '..';\n");
            output.push_str(&generator.indent());
            output.push_str(&format!("push @{}, $file;\n", array_name));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
            output.push_str(&generator.indent());
            output.push_str("closedir($dh);\n");
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
        }
    }
    
    output
}

pub fn generate_ls_command(generator: &mut Generator, cmd: &SimpleCommand, pipeline_context: bool, output_var: Option<&str>) -> String {
    let mut output = String::new();
    
    // Parse ls arguments to determine directory and flags
    let mut dir = ".";
    let mut single_column = false;
    
    for arg in &cmd.args {
        if let Word::Literal(s, _) = arg {
            if s.starts_with('-') {
                // Parse flags
                for flag in s.chars().skip(1) {
                    match flag {
                        '1' => single_column = true,
                        _ => {} // Ignore other flags for now
                    }
                }
            } else {
                // This is a directory argument
                dir = s;
            }
        }
    }
    
    // Handle context-based logic
    if pipeline_context {
        // Pipeline context: populate array but don't print - output goes to pipeline
        // Don't sort by default - only sort when explicitly requested with -C or -x flags
        let should_sort = false; // Default to no sorting to match shell behavior
        output.push_str(&generate_ls_helper(generator, dir, "ls_files", should_sort));
        if let Some(var) = output_var {
            output.push_str(&generator.indent());
            output.push_str(&format!("${} = join(\"\\n\", @ls_files);\n", var));
            // Ensure output ends with newline to match shell behavior
            output.push_str(&generator.indent());
            output.push_str(&format!("${} .= \"\\n\" unless ${} =~ /\\n$/;\n", var, var));
        }
        // No print statement in pipeline context
    } else {
        // Standalone ls command: print files
        if single_column {
            // -1 flag: one file per line, preserve directory order (no sorting)
            output.push_str(&generate_ls_helper(generator, dir, "ls_files", false));
            output.push_str(&generator.indent());
            output.push_str("print join(\"\\n\", @ls_files) . \"\\n\";\n");
        } else {
            // Default: one file per line (like ls -1) for predictable output
            output.push_str(&generate_ls_helper(generator, dir, "ls_files", false));
            output.push_str(&generator.indent());
            output.push_str("print join(\"\\n\", @ls_files) . \"\\n\";\n");
        }
    }
    
    output
}

pub fn generate_ls_for_substitution(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    // Parse ls arguments to determine directory and flags
    let mut dir = ".";
    let mut single_column = false; // Default to multi-column (space-separated) like shell ls
    
    for arg in &cmd.args {
        if let Word::Literal(s, _) = arg {
            if s.starts_with('-') {
                // Parse flags
                for flag in s.chars().skip(1) {
                    match flag {
                        '1' => single_column = true,  // -1 flag: explicit single column (newline-separated)
                        'C' => single_column = false, // -C flag: multi-column (space-separated)
                        'x' => single_column = false, // -x flag: multi-column (space-separated)
                        _ => {} // Ignore other flags for now
                    }
                }
            } else {
                // This is a directory argument (not a flag)
                dir = s;
            }
        }
    }
    
    let mut output = String::new();
    output.push_str("do {\n");
    generator.indent_level += 1;
    // Use sorting based on flags, not always true
    let should_sort = !single_column; // Only sort if not using -1 flag
    output.push_str(&generate_ls_helper(generator, dir, "ls_files_sub", should_sort));
    output.push_str(&generator.indent());
    if single_column {
        // -1 flag: join with newlines to preserve one file per line
        output.push_str("join(\"\\n\", @ls_files_sub);\n");
    } else {
        // Default behavior and -C or -x flags: join with spaces for multi-column output
        output.push_str("join(\" \", @ls_files_sub);\n");
    }
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}");
    
    output
}
