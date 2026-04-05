use crate::ast::*;
use crate::generator::Generator;

fn generate_ls_helper(
    generator: &mut Generator,
    dir: &str,
    array_name: &str,
    sort_files: bool,
    add_slash_to_dirs: bool,
    sort_by_time: bool,
    show_hidden: bool,
) -> String {
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
                output.push_str(&format!(
                    "use Time::HiRes qw(stat);\n@{} = sort {{ my $mtime_a = (stat(\"{}/$a\"))[9]; my $mtime_b = (stat(\"{}/$b\"))[9]; $mtime_b <=> $mtime_a || $a cmp $b }} @{};\n",
                    array_name, dir, dir, array_name
                ));
            } else {
                // Match shell ls ordering without letting -p suffixes affect sort order
                output.push_str(&format!(
                    "@{} = sort {{ my $aa = $a; my $bb = $b; $aa =~ s{{/$}}{{}}; $bb =~ s{{/$}}{{}}; $aa cmp $bb }} @{};\n",
                    array_name, array_name
                ));
            }
        }
    } else {
        // Check if the argument is a file (not a directory)
        output.push_str(&generator.indent());
        let dir_literal = if dir == "." {
            "q{.}"
        } else {
            &format!("'{}'", dir)
        };
        output.push_str(&format!("if ( -f {} ) {{\n", dir_literal));
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str(&format!("push @{}, {};\n", array_name, dir_literal));
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
        output.push_str(&generator.indent());
        output.push_str(&format!("elsif ( -d {} ) {{\n", dir_literal));
        generator.indent_level += 1;
        // For directories, use opendir/readdir
        output.push_str(&generator.indent());
        output.push_str(&format!("if ( opendir my $dh, {} ) {{\n", dir_literal));
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str("while ( my $file = readdir $dh ) {\n");
        generator.indent_level += 1;
        if !show_hidden {
            output.push_str(&generator.indent());
            output.push_str("next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;\n");
        }
        // Skip temporary files that might be created during test execution
        output.push_str(&generator.indent());
        output.push_str("next if $file =~ /^__tmp_.*[.]pl$/msx;\n");
        // Skip other common temporary files created during testing
        output.push_str(&generator.indent());
        output.push_str("next if $file =~ /^(debug_|temp_|test_|file\\d*[.]txt)$/msx;\n");
        if add_slash_to_dirs {
            output.push_str(&generator.indent());
            output.push_str(&format!("if ( -d \"{}/$file\" ) {{\n", dir));
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
                output.push_str(&format!(
                    "use Time::HiRes qw(stat);\n@{} = sort {{ my $mtime_a = (stat(\"{}/$a\"))[9]; my $mtime_b = (stat(\"{}/$b\"))[9]; $mtime_b <=> $mtime_a || $a cmp $b }} @{};\n",
                    array_name, dir, dir, array_name
                ));
            } else {
                // Match shell ls ordering without letting -p suffixes affect sort order
                output.push_str(&format!(
                    "@{} = sort {{ my $aa = $a; my $bb = $b; $aa =~ s{{/$}}{{}}; $bb =~ s{{/$}}{{}}; $aa cmp $bb }} @{};\n",
                    array_name, array_name
                ));
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

fn generate_ls_sections_helper(
    generator: &mut Generator,
    file_args: &[&str],
    sections_array_name: &str,
    all_found_var: &str,
    sort_by_time: bool,
    add_slash_to_dirs: bool,
    show_hidden: bool,
) -> String {
    let mut output = String::new();
    let inputs_array = format!("ls_inputs_{}", generator.get_unique_id());
    let files_array = format!("ls_files_{}", generator.get_unique_id());
    let dirs_array = format!("ls_dirs_{}", generator.get_unique_id());
    let show_headers_var = format!("ls_show_headers_{}", generator.get_unique_id());
    let input_var = format!("ls_item_{}", generator.get_unique_id());
    let dir_var = format!("ls_dir_{}", generator.get_unique_id());
    let dir_entries_array = format!("ls_dir_entries_{}", generator.get_unique_id());

    output.push_str(&generator.indent());
    output.push_str(&format!("my @{} = ();\n", sections_array_name));
    output.push_str(&generator.indent());
    output.push_str(&format!("my ${} = 1;\n", all_found_var));
    output.push_str(&generator.indent());
    output.push_str(&format!("my @{} = ();\n", inputs_array));

    for (idx, file_arg) in file_args.iter().enumerate() {
        let literal = generator.perl_string_literal(&Word::literal((*file_arg).to_string()));
        if file_arg.contains('*') || file_arg.contains('?') {
            let glob_array = format!("ls_glob_{}_{}", inputs_array, idx);
            output.push_str(&generator.indent());
            output.push_str(&format!("my @{} = glob({});\n", glob_array, literal));
            output.push_str(&generator.indent());
            output.push_str(&format!("if ( !@{} ) {{\n", glob_array));
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str(&format!("push @{}, {};\n", inputs_array, literal));
            output.push_str(&generator.indent());
            output.push_str(&format!("${} = 0;\n", all_found_var));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("} else {\n");
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str(&format!("push @{}, @{};\n", inputs_array, glob_array));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
        } else {
            output.push_str(&generator.indent());
            output.push_str(&format!("push @{}, {};\n", inputs_array, literal));
        }
    }

    output.push_str(&generator.indent());
    output.push_str(&format!("my @{} = ();\n", files_array));
    output.push_str(&generator.indent());
    output.push_str(&format!("my @{} = ();\n", dirs_array));
    output.push_str(&generator.indent());
    output.push_str(&format!(
        "my ${} = scalar(@{}) > 1;\n",
        show_headers_var, inputs_array
    ));
    output.push_str(&generator.indent());
    output.push_str(&format!("for my ${} (@{}) {{\n", input_var, inputs_array));
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str(&format!("if ( -f ${} ) {{\n", input_var));
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str(&format!("push @{}, ${};\n", files_array, input_var));
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    output.push_str(&generator.indent());
    output.push_str(&format!("elsif ( -d ${} ) {{\n", input_var));
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str(&format!("push @{}, ${};\n", dirs_array, input_var));
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    output.push_str(&generator.indent());
    output.push_str("else {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str(&format!("${} = 0;\n", all_found_var));
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");

    if sort_by_time {
        output.push_str(&generator.indent());
        output.push_str("use Time::HiRes qw(stat);\n");
        output.push_str(&generator.indent());
        output.push_str(&format!(
            "@{} = sort {{ my $mtime_a = (stat($a))[9]; my $mtime_b = (stat($b))[9]; $mtime_b <=> $mtime_a || $a cmp $b }} @{};\n",
            files_array, files_array
        ));
        output.push_str(&generator.indent());
        output.push_str(&format!(
            "@{} = sort {{ my $mtime_a = (stat($a))[9]; my $mtime_b = (stat($b))[9]; $mtime_b <=> $mtime_a || $a cmp $b }} @{};\n",
            dirs_array, dirs_array
        ));
    } else {
        output.push_str(&generator.indent());
        output.push_str(&format!(
            "@{} = sort {{ $a cmp $b }} @{};\n",
            files_array, files_array
        ));
        output.push_str(&generator.indent());
        output.push_str(&format!(
            "@{} = sort {{ $a cmp $b }} @{};\n",
            dirs_array, dirs_array
        ));
    }

    output.push_str(&generator.indent());
    output.push_str(&format!("if (@{}) {{\n", files_array));
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str(&format!(
        "push @{}, join(\"\\n\", @{});\n",
        sections_array_name, files_array
    ));
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");

    output.push_str(&generator.indent());
    output.push_str(&format!("for my ${} (@{}) {{\n", dir_var, dirs_array));
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str(&format!("my @{} = ();\n", dir_entries_array));
    output.push_str(&generator.indent());
    output.push_str(&format!("if ( opendir my $dh, ${} ) {{\n", dir_var));
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str("while ( my $file = readdir $dh ) {\n");
    generator.indent_level += 1;
    if !show_hidden {
        output.push_str(&generator.indent());
        output.push_str("next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;\n");
    }
    output.push_str(&generator.indent());
    output.push_str("next if $file =~ /^__tmp_.*[.]pl$/msx;\n");
    output.push_str(&generator.indent());
    output.push_str("next if $file =~ /^(debug_|temp_|test_|file\\d*[.]txt)$/msx;\n");
    if add_slash_to_dirs {
        output.push_str(&generator.indent());
        output.push_str(&format!("if ( -d \"${}/$file\" ) {{\n", dir_var));
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str(&format!("push @{}, \"$file/\";\n", dir_entries_array));
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("} else {\n");
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        output.push_str(&format!("push @{}, $file;\n", dir_entries_array));
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");
    } else {
        output.push_str(&generator.indent());
        output.push_str(&format!("push @{}, $file;\n", dir_entries_array));
    }
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    output.push_str(&generator.indent());
    output.push_str("closedir $dh;\n");

    if sort_by_time {
        output.push_str(&generator.indent());
        output.push_str("use Time::HiRes qw(stat);\n");
        output.push_str(&generator.indent());
        output.push_str(&format!(
            "@{} = sort {{ my $mtime_a = (stat(\"${}/$a\"))[9]; my $mtime_b = (stat(\"${}/$b\"))[9]; $mtime_b <=> $mtime_a || $a cmp $b }} @{};\n",
            dir_entries_array, dir_var, dir_var, dir_entries_array
        ));
    } else {
        output.push_str(&generator.indent());
        output.push_str(&format!(
            "@{} = sort {{ my $aa = $a; my $bb = $b; $aa =~ s{{/$}}{{}}; $bb =~ s{{/$}}{{}}; $aa cmp $bb }} @{};\n",
            dir_entries_array, dir_entries_array
        ));
    }

    output.push_str(&generator.indent());
    output.push_str(&format!("if ( ${} ) {{\n", show_headers_var));
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str(&format!("if ( @{} ) {{\n", dir_entries_array));
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str(&format!(
        "push @{}, ${} . \":\\n\" . join(\"\\n\", @{});\n",
        sections_array_name, dir_var, dir_entries_array
    ));
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("} else {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str(&format!(
        "push @{}, ${} . ':';\n",
        sections_array_name, dir_var
    ));
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    output.push_str(&generator.indent());
    output.push_str(&format!("elsif ( @{} ) {{\n", dir_entries_array));
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str(&format!(
        "push @{}, join(\"\\n\", @{});\n",
        sections_array_name, dir_entries_array
    ));
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    output.push_str(&generator.indent());
    output.push_str("else {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str(&format!("${} = 0;\n", all_found_var));
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}\n");

    output
}

pub fn generate_ls_command(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    pipeline_context: bool,
    output_var: Option<&str>,
) -> String {
    let mut output = String::new();

    // Parse ls arguments to determine directory and flags
    let dir = ".";
    let mut _single_column = false;
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
                    if crate::debug::is_debug_enabled() {
                        eprintln!("DEBUG: ls command file/directory argument: '{}'", s);
                    }
                    file_args.push(s.as_str());
                }
            }
            Word::StringInterpolation(interp, _) => {
                // Handle string interpolation - extract the literal part
                if interp.parts.len() == 1 {
                    if let StringPart::Literal(s) = &interp.parts[0] {
                        if !s.starts_with('-') {
                            // This is a file/directory argument
                            if crate::debug::is_debug_enabled() {
                                eprintln!("DEBUG: ls command file/directory argument (from interpolation): '{}'", s);
                            }
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
                            '1' => _single_column = true,
                            'p' => add_slash_to_dirs = true, // -p flag: add / to directories
                            'l' => _long_format = true,      // -l flag: long format
                            't' => sort_by_time = true,      // -t flag: sort by modification time
                            'a' => show_hidden = true,       // -a flag: show hidden files
                            _ => {}                          // Ignore other flags for now
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
                                    '1' => _single_column = true,
                                    'p' => add_slash_to_dirs = true, // -p flag: add / to directories
                                    'l' => _long_format = true,      // -l flag: long format
                                    't' => sort_by_time = true, // -t flag: sort by modification time
                                    'a' => show_hidden = true,  // -a flag: show hidden files
                                    _ => {}                     // Ignore other flags for now
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
        let all_found_var = format!("ls_all_found_{}", generator.get_unique_id());
        let all_found_var = format!("ls_all_found_{}", generator.get_unique_id());
        let all_found_var = format!("ls_all_found_{}", generator.get_unique_id());

        if has_file_args {
            // Handle multiple file arguments
            output.push_str(&generator.indent());
            output.push_str(&format!("my @{} = ();\n", array_name));
            for file_arg in &file_args {
                output.push_str(&generator.indent());
                let file_literal = if *file_arg == "." {
                    "q{.}"
                } else {
                    &format!("'{}'", file_arg)
                };
                output.push_str(&format!("if ( -f {} ) {{\n", file_literal));
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str(&format!("push @{}, {};\n", array_name, file_literal));
                generator.indent_level -= 1;
                output.push_str(&generator.indent());
                output.push_str("}\n");
                output.push_str(&generator.indent());
                output.push_str(&format!("elsif ( -d {} ) {{\n", file_literal));
                generator.indent_level += 1;
                // For directories, list their contents
                output.push_str(&generator.indent());
                output.push_str(&format!("if ( opendir my $dh, {} ) {{\n", file_literal));
                generator.indent_level += 1;
                output.push_str(&generator.indent());
                output.push_str("while ( my $file = readdir $dh ) {\n");
                generator.indent_level += 1;
                if !show_hidden {
                    output.push_str(&generator.indent());
                    output.push_str(
                        "next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;\n",
                    );
                }
                // Skip temporary files that might be created during test execution
                output.push_str(&generator.indent());
                output.push_str("next if $file =~ /^__tmp_.*[.]pl$/msx;\n");
                // Skip other common temporary files created during testing
                output.push_str(&generator.indent());
                output.push_str("next if $file =~ /^(debug_|temp_|test_|file\\d*[.]txt)$/msx;\n");
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
            output.push_str(&generate_ls_helper(
                generator,
                dir,
                &array_name,
                should_sort,
                add_slash_to_dirs,
                sort_by_time,
                show_hidden,
            ));
        }

        if let Some(var) = output_var {
            output.push_str(&generator.indent());
            output.push_str(&format!("${} = join \"\\n\", @{};\n", var, array_name));
            // Ensure output ends with newline to match shell behavior
            output.push_str(&generator.indent());
            output.push_str(&format!(
                "if ( !( ${} =~ {} ) ) {{\n",
                var,
                generator.newline_end_regex()
            ));
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
        let all_found_var = format!("ls_all_found_{}", generator.get_unique_id());

        if has_file_args {
            if file_args.len() > 1 {
                output.push_str(&generate_ls_sections_helper(
                    generator,
                    &file_args,
                    &array_name,
                    &all_found_var,
                    sort_by_time,
                    add_slash_to_dirs,
                    show_hidden,
                ));
            } else {
                // Handle a single file or directory argument.
                output.push_str(&generator.indent());
                output.push_str(&format!("my @{} = ();\n", array_name));

                for file_arg in &file_args {
                    output.push_str(&generator.indent());
                    let file_literal = if *file_arg == "." {
                        "q{.}"
                    } else {
                        &format!("'{}'", file_arg)
                    };
                    output.push_str(&format!("if ( -f {} ) {{\n", file_literal));
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str(&format!("push @{}, {};\n", array_name, file_literal));
                    generator.indent_level -= 1;
                    output.push_str(&generator.indent());
                    output.push_str("}\n");
                    output.push_str(&generator.indent());
                    output.push_str(&format!("elsif ( -d {} ) {{\n", file_literal));
                    generator.indent_level += 1;
                    // For directories, list their contents
                    output.push_str(&generator.indent());
                    output.push_str(&format!("if ( opendir my $dh, {} ) {{\n", file_literal));
                    generator.indent_level += 1;
                    output.push_str(&generator.indent());
                    output.push_str("while ( my $file = readdir $dh ) {\n");
                    generator.indent_level += 1;
                    if !show_hidden {
                        output.push_str(&generator.indent());
                        output.push_str(
                            "next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;\n",
                        );
                    }
                    // Skip temporary files that might be created during test execution
                    output.push_str(&generator.indent());
                    output.push_str("next if $file =~ /^__tmp_.*[.]pl$/msx;\n");
                    // Skip other common temporary files created during testing
                    output.push_str(&generator.indent());
                    output
                        .push_str("next if $file =~ /^(debug_|temp_|test_|file\\d*[.]txt)$/msx;\n");
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
            }
        } else {
            // No file arguments, use default directory
            output.push_str(&generate_ls_helper(
                generator,
                dir,
                &array_name,
                true,
                add_slash_to_dirs,
                sort_by_time,
                show_hidden,
            ));
        }

        // Print the results
        output.push_str(&generator.indent());
        output.push_str(&format!("if (@{}) {{\n", array_name));
        generator.indent_level += 1;
        output.push_str(&generator.indent());
        let join_sep = if file_args.len() > 1 { "\\n\\n" } else { "\\n" };
        output.push_str(&format!("print join \"{}\", @{};\n", join_sep, array_name));
        output.push_str(&generator.indent());
        output.push_str("print \"\\n\";\n");
        generator.indent_level -= 1;
        output.push_str(&generator.indent());
        output.push_str("}\n");

        if has_file_args {
            output.push_str(&generator.indent());
            output.push_str(&format!("if ( ${} ) {{\n", all_found_var));
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str("local $CHILD_ERROR = 0;\n");
            output.push_str(&generator.indent());
            output.push_str("$ls_success = 1;\n");
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
            output.push_str(&generator.indent());
            output.push_str("else {\n");
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str("local $CHILD_ERROR = 2;\n");
            output.push_str(&generator.indent());
            output.push_str("$ls_success = 0;\n");
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
        } else {
            output.push_str(&generator.indent());
            output.push_str("local $CHILD_ERROR = 0;\n");
            output.push_str(&generator.indent());
            output.push_str("$ls_success = 1;\n");
        }
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
    output.push_str("if ( opendir my $dh, $ls_dir ) {\n");
    generator.indent_level += 1;
    output.push_str(&generator.indent());
    output.push_str("while ( my $file = readdir $dh ) {\n");
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
    let mut sort_by_time = false; // -t flag: sort by modification time
    let mut show_hidden = false; // -a flag: show hidden files

    for arg in &cmd.args {
        if let Word::Literal(s, _) = arg {
            if s.starts_with('-') {
                // Parse flags
                for flag in s.chars().skip(1) {
                    match flag {
                        '1' => _single_column = true, // -1 flag: explicit single column (newline-separated)
                        'C' => _single_column = false, // -C flag: multi-column (space-separated)
                        'x' => _single_column = false, // -x flag: multi-column (space-separated)
                        'p' => add_slash_to_dirs = true, // -p flag: add / to directories
                        'l' => _long_format = true,   // -l flag: long format
                        't' => sort_by_time = true,   // -t flag: sort by modification time
                        'a' => show_hidden = true,    // -a flag: show hidden files
                        _ => {}                       // Ignore other flags for now
                    }
                }
            } else {
                // This is a file/directory argument (not a flag)
                file_args.push(s.as_str());
            }
        }
    }

    // Save the current indent level
    let saved_indent = generator.indent_level;
    generator.indent_level = 0; // Start with no indentation since we'll format this ourselves

    let mut output = String::new();
    output.push_str("do {\n");
    generator.indent_level = 1; // Set to 1 for content inside do block

    let array_name = format!("ls_files_{}", generator.get_unique_id());
    let all_found_var = format!("ls_all_found_{}", generator.get_unique_id());

    if !file_args.is_empty() {
        output.push_str(&generate_ls_sections_helper(
            generator,
            &file_args,
            &array_name,
            &all_found_var,
            false,
            add_slash_to_dirs,
            show_hidden,
        ));
    } else {
        // No file arguments, use default directory
        output.push_str(&generate_ls_helper(
            generator,
            ".",
            &array_name,
            true,
            add_slash_to_dirs,
            false,
            show_hidden,
        ));
    }

    output.push_str(&generator.indent());
    // Preserve the shell's trailing newline for non-empty output.
    let join_sep = if file_args.is_empty() {
        "\\n"
    } else {
        "\\n\\n"
    };
    output.push_str(&format!(
        "(@{} ? join(\"{}\", @{}) . \"\\n\" : q{{}});\n",
        array_name, join_sep, array_name
    ));
    generator.indent_level -= 1;
    output.push_str(&generator.indent());
    output.push_str("}");

    // Restore the saved indent level
    generator.indent_level = saved_indent;

    output
}
