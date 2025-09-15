use crate::ast::*;
use crate::generator::Generator;

fn generate_ls_helper(generator: &mut Generator, dir: &str, array_name: &str, sort_files: bool, add_slash_to_dirs: bool, sort_by_time: bool, show_hidden: bool) -> String {
    let mut output = String::new();
    
    // Declare the array
    output.push_str(&generator.indent());
    output.push_str(&format!("my @{} = ();\n", array_name));
    
    // Check if this is a glob pattern (contains * or ?)
    let is_glob = dir.contains('*') || dir.contains('?');
    
    if is_glob {
        // For glob patterns, use Perl's glob function
        output.push_str(&generator.indent());
        output.push_str(&format!("@{} = glob('{}');\n", array_name, dir));
        
        if sort_files {
            output.push_str(&generator.indent());
            if sort_by_time {
                output.push_str(&format!("@{} = sort {{ -M \"{}/$b\" <=> -M \"{}/$a\" }} @{};\n", array_name, dir, dir, array_name));
            } else {
                // Simple alphabetical sorting to match native ls behavior
                output.push_str(&format!("@{} = sort {{ $a cmp $b }} @{};\n", array_name, array_name));
            }
        }
    } else {
        // Check if the argument is a file (not a directory)
        output.push_str(&generator.indent());
        output.push_str(&format!("if (-f '{}') {{\n", dir));
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str(&format!("push @{}, '{}';\n", array_name, dir));
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str(&format!("}} elsif (-d '{}') {{\n", dir));
        generator.indent_level += 1;
        // For directories, use opendir/readdir
        output.push_str(&generator.indent());
        let dir_literal = if dir == "." { "q{.}" } else { &format!("'{}'", dir) };
        output.push_str(&format!("if (opendir my $dh, {}) {{\n", dir_literal));
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str("while (my $file = readdir $dh) {\n");
        generator.indent_level += 1;
        if !show_hidden {
            output.push_str(&generator.indent());
            output.push_str("next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;\n");
        }
        if add_slash_to_dirs {
            output.push_str(&generator.indent());
            output.push_str(&format!("if (-d \"{}/$file\") {{\n", dir));
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str(&format!("push @{}, \"$file/\";\n", array_name));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("} else {\n");
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str(&format!("push @{}, $file;\n", array_name));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
        } else {
            output.push_str(&generator.indent());
            output.push_str(&format!("push @{}, $file;\n", array_name));
        }
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
        output.push_str(&generator.indent());
        output.push_str("closedir $dh;\n");
        
        if sort_files {
            output.push_str(&generator.indent());
            if sort_by_time {
                output.push_str(&format!("@{} = sort {{ -M \"{}/$b\" <=> -M \"{}/$a\" }} @{};\n", array_name, dir, dir, array_name));
            } else {
                // Simple alphabetical sorting to match native ls behavior
                output.push_str(&format!("@{} = sort {{ $a cmp $b }} @{};\n", array_name, array_name));
            }
        }
        
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
    }
    
    output
}

pub fn generate_ls_command(generator: &mut Generator, cmd: &SimpleCommand, pipeline_context: bool, output_var: Option<&str>) -> String {
    let mut output = String::new();
    
    // Debug messages removed for cleaner output
    
    // Parse ls arguments to determine directory and flags
    let mut dir = ".";
    let mut single_column = false;
    let mut add_slash_to_dirs = false; // -p flag: add / to directories
    let mut _long_format = false; // -l flag: long format
    let mut sort_by_time = false; // -t flag: sort by modification time
    let mut show_hidden = false; // -a flag: show hidden files
    
    // First pass: collect all file/directory arguments
    let mut file_args = Vec::new();
    for arg in &cmd.args {
        match arg {
            Word::Literal(s, _) => {
                if !s.starts_with('-') {
                    // This is a file/directory argument
                    eprintln!("DEBUG: ls command file/directory argument: '{}'", s);
                    file_args.push(s.as_str());
                }
            }
            Word::StringInterpolation(interp, _) => {
                // Handle string interpolation - extract the literal part
                if interp.parts.len() == 1 {
                    if let StringPart::Literal(s) = &interp.parts[0] {
                        if !s.starts_with('-') {
                            // This is a file/directory argument
                            eprintln!("DEBUG: ls command file/directory argument (from interpolation): '{}'", s);
                            file_args.push(s.as_str());
                        }
                    }
                }
            }
            _ => {} // Ignore other argument types for now
        }
    }
    
    // If we have file arguments, we need to handle them differently
    let has_file_args = !file_args.is_empty();
    
    // Second pass: parse flags
    for arg in &cmd.args {
        match arg {
            Word::Literal(s, _) => {
                if s.starts_with('-') {
                    // Parse flags
                    for flag in s.chars().skip(1) {
                        match flag {
                            '1' => single_column = true,
                            'p' => add_slash_to_dirs = true, // -p flag: add / to directories
                            'l' => _long_format = true, // -l flag: long format
                            't' => sort_by_time = true, // -t flag: sort by modification time
                            'a' => show_hidden = true, // -a flag: show hidden files
                            _ => {} // Ignore other flags for now
                        }
                    }
                }
            }
            Word::StringInterpolation(interp, _) => {
                // Handle string interpolation - extract the literal part
                if interp.parts.len() == 1 {
                    if let StringPart::Literal(s) = &interp.parts[0] {
                        if s.starts_with('-') {
                            // Parse flags
                            for flag in s.chars().skip(1) {
                                match flag {
                                    '1' => single_column = true,
                                    'p' => add_slash_to_dirs = true, // -p flag: add / to directories
                                    'l' => _long_format = true, // -l flag: long format
                                    't' => sort_by_time = true, // -t flag: sort by modification time
                                    'a' => show_hidden = true, // -a flag: show hidden files
                                    _ => {} // Ignore other flags for now
                                }
                            }
                        }
                    }
                }
            }
            _ => {} // Ignore other argument types for now
        }
    }
    // Debug message removed for cleaner output
    
    // Handle context-based logic
    if pipeline_context {
        // Pipeline context: populate array but don't print - output goes to pipeline
        let should_sort = true; // Default to sorting to match shell behavior
        let array_name = format!("ls_files_{}", generator.get_unique_id());
        
        if has_file_args {
            // Handle multiple file arguments
            output.push_str(&generator.indent());
            output.push_str(&format!("my @{} = ();\n", array_name));
            for file_arg in &file_args {
                output.push_str(&generator.indent());
                output.push_str(&format!("if (-f '{}') {{\n", file_arg));
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&format!("push @{}, '{}';\n", array_name, file_arg));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str(&format!("}} elsif (-d '{}') {{\n", file_arg));
                generator.indent_level += 1;
                // For directories, list their contents
                output.push_str(&generator.indent());
                output.push_str(&format!("if (opendir my $dh, '{}') {{\n", file_arg));
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str("while (my $file = readdir $dh) {\n");
                generator.indent_level += 1;
                if !show_hidden {
                    output.push_str(&generator.indent());
                    output.push_str("next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;\n");
                }
                if add_slash_to_dirs {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("if (-d \"{}/$file\") {{\n", file_arg));
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str(&format!("push @{}, \"$file/\";\n", array_name));
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("} else {\n");
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str(&format!("push @{}, $file;\n", array_name));
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                } else {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("push @{}, $file;\n", array_name));
                }
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                output.push_str(&generator.indent());
                output.push_str("closedir $dh;\n");
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
            }
        } else {
            // No file arguments, use default directory
            output.push_str(&generate_ls_helper(generator, dir, &array_name, should_sort, add_slash_to_dirs, sort_by_time, show_hidden));
        }
        
        if let Some(var) = output_var {
            output.push_str(&generator.indent());
            output.push_str(&format!("${} = join \"\\n\", @{};\n", var, array_name));
            // Ensure output ends with newline to match shell behavior
            output.push_str(&generator.indent());
            output.push_str(&format!("if (!(${} =~ {})) {{\n", var, generator.newline_end_regex()));
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str(&format!("${} .= \"\\n\";\n", var));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
        }
        // No print statement in pipeline context
    } else {
        // Standalone ls command: print files
        let array_name = format!("ls_files_{}", generator.get_unique_id());
        
        if has_file_args {
            // Handle multiple file arguments
            output.push_str(&generator.indent());
            output.push_str(&format!("my @{} = ();\n", array_name));
            for file_arg in &file_args {
                output.push_str(&generator.indent());
                output.push_str(&format!("if (-f '{}') {{\n", file_arg));
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&format!("push @{}, '{}';\n", array_name, file_arg));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str(&format!("}} elsif (-d '{}') {{\n", file_arg));
                generator.indent_level += 1;
                // For directories, list their contents
                output.push_str(&generator.indent());
                output.push_str(&format!("if (opendir my $dh, '{}') {{\n", file_arg));
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str("while (my $file = readdir $dh) {\n");
                generator.indent_level += 1;
                if !show_hidden {
                    output.push_str(&generator.indent());
                    output.push_str("next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;\n");
                }
                if add_slash_to_dirs {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("if (-d \"{}/$file\") {{\n", file_arg));
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str(&format!("push @{}, \"$file/\";\n", array_name));
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("} else {\n");
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str(&format!("push @{}, $file;\n", array_name));
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                } else {
                    output.push_str(&generator.indent());
                    output.push_str(&format!("push @{}, $file;\n", array_name));
                }
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                output.push_str(&generator.indent());
                output.push_str("closedir $dh;\n");
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
            }
        } else {
            // No file arguments, use default directory
            output.push_str(&generate_ls_helper(generator, dir, &array_name, true, add_slash_to_dirs, sort_by_time, show_hidden));
        }
        
        // Print the results
        output.push_str(&generator.indent());
        output.push_str(&format!("if (@{}) {{\n", array_name));
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str(&format!("print join \"\\n\", @{}, \"\\n\";\n", array_name));
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
    }
    
    output
}

pub fn generate_ls_preamble(generator: &mut Generator) -> String {
    let mut output = String::new();
    
    // Generate a generic preamble that can handle any directory
    output.push_str(&generator.indent());
    output.push_str("my @ls_files_preamble;\n");
    output.push_str(&generator.indent());
    output.push_str("my $ls_dir;\n");
    output.push_str(&generator.indent());
    output.push_str("if (opendir my $dh, $ls_dir) {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str("while (my $file = readdir $dh) {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str("next if $file eq q{.} || $file eq q{..};\n");
    output.push_str(&generator.indent());
    output.push_str("push @ls_files_preamble, $file;\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    output.push_str(&generator.indent());
    output.push_str("closedir $dh;\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    
    output
}

pub fn generate_ls_for_substitution(generator: &mut Generator, cmd: &SimpleCommand) -> String {
    // Parse ls arguments to determine files/directories and flags
    let mut file_args = Vec::new();
    let mut _single_column = false; // Default to multi-column (space-separated) like shell ls
    let mut add_slash_to_dirs = false; // -p flag: add / to directories
    let mut _long_format = false; // -l flag: long format
    let mut show_hidden = false; // -a flag: show hidden files
    
    for arg in &cmd.args {
        if let Word::Literal(s, _) = arg {
            if s.starts_with('-') {
                // Parse flags
                for flag in s.chars().skip(1) {
                    match flag {
                        '1' => _single_column = true,  // -1 flag: explicit single column (newline-separated)
                        'C' => _single_column = false, // -C flag: multi-column (space-separated)
                        'x' => _single_column = false, // -x flag: multi-column (space-separated)
                        'p' => add_slash_to_dirs = true, // -p flag: add / to directories
                        'l' => _long_format = true, // -l flag: long format
                        'a' => show_hidden = true, // -a flag: show hidden files
                        _ => {} // Ignore other flags for now
                    }
                }
            } else {
                // This is a file/directory argument (not a flag)
                file_args.push(s.as_str());
            }
        }
    }
    
    let mut output = String::new();
    output.push_str("do {\n");
    generator.indent_level += 1;
    
    let array_name = format!("ls_files_{}", generator.get_unique_id());
    
    if !file_args.is_empty() {
        // Handle multiple file arguments
        output.push_str(&generator.indent());
        output.push_str(&format!("my @{} = ();\n", array_name));
        for file_arg in &file_args {
            output.push_str(&generator.indent());
            output.push_str(&format!("if (-f '{}') {{\n", file_arg));
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str(&format!("push @{}, '{}';\n", array_name, file_arg));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str(&format!("}} elsif (-d '{}') {{\n", file_arg));
            generator.indent_level += 1;
            // For directories, list their contents
            output.push_str(&generator.indent());
            output.push_str(&format!("if (opendir my $dh, '{}') {{\n", file_arg));
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str("while (my $file = readdir $dh) {\n");
            generator.indent_level += 1;
            if !show_hidden {
                output.push_str(&generator.indent());
                output.push_str("next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;\n");
            }
            if add_slash_to_dirs {
                output.push_str(&generator.indent());
                output.push_str(&format!("if (-d \"{}/$file\") {{\n", file_arg));
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&format!("push @{}, \"$file/\";\n", array_name));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("} else {\n");
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&format!("push @{}, $file;\n", array_name));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
            } else {
                output.push_str(&generator.indent());
                output.push_str(&format!("push @{}, $file;\n", array_name));
            }
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
            output.push_str(&generator.indent());
            output.push_str("closedir $dh;\n");
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
        }
    } else {
        // No file arguments, use default directory
        output.push_str(&generate_ls_helper(generator, ".", &array_name, true, add_slash_to_dirs, false, show_hidden));
    }
    
    output.push_str(&generator.indent());
    // In command substitution context, always join with newlines to match shell behavior
    // The shell's ls command outputs one file per line by default in command substitution
    output.push_str(&format!("join \"\\n\", @{};\n", array_name));
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}");
    
    output
}
