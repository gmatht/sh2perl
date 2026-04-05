use crate::ast::*;
use crate::generator::Generator;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct BuiltinCommand {
    pub name: &'static str,
    pub description: &'static str,
    pub supports_linebyline: bool,
}

impl BuiltinCommand {
    pub fn new(name: &'static str, description: &'static str, supports_linebyline: bool) -> Self {
        Self {
            name,
            description,
            supports_linebyline,
        }
    }
}

pub fn get_builtin_commands() -> HashMap<&'static str, BuiltinCommand> {
    let mut commands = HashMap::new();

    // File and directory operations
    commands.insert(
        "ls",
        BuiltinCommand::new("ls", "List directory contents", false),
    );
    commands.insert(
        "cat",
        BuiltinCommand::new("cat", "Concatenate and display files", true),
    );
    commands.insert("find", BuiltinCommand::new("find", "Find files", true));
    commands.insert(
        "grep",
        BuiltinCommand::new("grep", "Search for patterns in text", true),
    );
    commands.insert("sed", BuiltinCommand::new("sed", "Stream editor", true));
    commands.insert(
        "awk",
        BuiltinCommand::new("awk", "Pattern scanning and processing", true),
    );
    commands.insert("sort", BuiltinCommand::new("sort", "Sort lines", false));
    commands.insert(
        "uniq",
        BuiltinCommand::new("uniq", "Remove duplicate lines", true),
    );
    commands.insert(
        "wc",
        BuiltinCommand::new("wc", "Word, line, and byte count", false),
    );
    commands.insert(
        "head",
        BuiltinCommand::new("head", "Display first lines", true),
    );
    commands.insert(
        "tail",
        BuiltinCommand::new("tail", "Display last lines", true),
    );
    commands.insert(
        "cut",
        BuiltinCommand::new("cut", "Cut sections from lines", true),
    );
    commands.insert(
        "paste",
        BuiltinCommand::new("paste", "Merge lines from files", false),
    );
    commands.insert(
        "comm",
        BuiltinCommand::new("comm", "Compare sorted files", false),
    );
    commands.insert("diff", BuiltinCommand::new("diff", "Compare files", false));
    commands.insert(
        "tr",
        BuiltinCommand::new("tr", "Translate or delete characters", true),
    );
    commands.insert(
        "xargs",
        BuiltinCommand::new("xargs", "Execute command with arguments", false),
    );
    commands.insert(
        "perl",
        BuiltinCommand::new("perl", "Perl interpreter", true),
    );
    commands.insert("cd", BuiltinCommand::new("cd", "Change directory", false));
    commands.insert(
        "read",
        BuiltinCommand::new("read", "Read input into variables", true),
    );

    // File manipulation
    commands.insert("cp", BuiltinCommand::new("cp", "Copy files", false));
    commands.insert("mv", BuiltinCommand::new("mv", "Move/rename files", false));
    commands.insert("rm", BuiltinCommand::new("rm", "Remove files", false));
    commands.insert(
        "mkdir",
        BuiltinCommand::new("mkdir", "Create directories", false),
    );
    commands.insert(
        "touch",
        BuiltinCommand::new("touch", "Create empty files", false),
    );

    // Text processing
    commands.insert("echo", BuiltinCommand::new("echo", "Display text", true));
    commands.insert(
        "printf",
        BuiltinCommand::new("printf", "Format and print data", true),
    );
    commands.insert(
        "basename",
        BuiltinCommand::new("basename", "Extract filename", true),
    );
    commands.insert(
        "dirname",
        BuiltinCommand::new("dirname", "Extract directory name", true),
    );

    // System utilities
    commands.insert(
        "pwd",
        BuiltinCommand::new("pwd", "Print working directory", false),
    );
    commands.insert(
        "seq",
        BuiltinCommand::new("seq", "Generate sequence of numbers", true),
    );
    commands.insert(
        "date",
        BuiltinCommand::new("date", "Display date and time", false),
    );
    commands.insert(
        "time",
        BuiltinCommand::new("time", "Time command execution", false),
    );
    commands.insert(
        "sleep",
        BuiltinCommand::new("sleep", "Delay execution", false),
    );
    commands.insert(
        "which",
        BuiltinCommand::new("which", "Locate command", false),
    );
    commands.insert(
        "yes",
        BuiltinCommand::new("yes", "Output string repeatedly", true),
    );
    commands.insert(
        "true",
        BuiltinCommand::new("true", "Return true (exit status 0)", false),
    );
    commands.insert(
        "false",
        BuiltinCommand::new("false", "Return false (exit status 1)", false),
    );

    // Compression and archiving
    commands.insert("gzip", BuiltinCommand::new("gzip", "Compress files", true));
    commands.insert(
        "zcat",
        BuiltinCommand::new("zcat", "Decompress and display", true),
    );

    // Network and downloads
    commands.insert("wget", BuiltinCommand::new("wget", "Download files", true));
    commands.insert("curl", BuiltinCommand::new("curl", "Transfer data", true));

    // Process management
    commands.insert(
        "kill",
        BuiltinCommand::new("kill", "Terminate processes", false),
    );
    commands.insert(
        "nohup",
        BuiltinCommand::new("nohup", "Run command immune to hangups", true),
    );
    commands.insert(
        "nice",
        BuiltinCommand::new("nice", "Run command with modified priority", true),
    );

    // Checksums and verification
    commands.insert(
        "sha256sum",
        BuiltinCommand::new("sha256sum", "Compute SHA256 checksums", true),
    );
    commands.insert(
        "sha512sum",
        BuiltinCommand::new("sha512sum", "Compute SHA512 checksums", true),
    );
    commands.insert(
        "strings",
        BuiltinCommand::new("strings", "Extract printable strings", false),
    );

    // I/O redirection
    commands.insert(
        "tee",
        BuiltinCommand::new("tee", "Read from stdin, write to stdout and files", true),
    );

    // Variable declarations
    commands.insert(
        "local",
        BuiltinCommand::new("local", "Declare local variables", false),
    );

    // Output generation
    commands.insert(
        "yes",
        BuiltinCommand::new("yes", "Output a string repeatedly", true),
    );

    //TODO: pkill and killall
    commands
}

pub fn is_builtin(command_name: &str) -> bool {
    get_builtin_commands().contains_key(command_name)
}

/// Check if all commands in a pipeline support line-by-line processing
pub fn pipeline_supports_linebyline(pipeline: &Pipeline) -> bool {
    // First check if all commands support line-by-line processing
    let all_support_linebyline = pipeline.commands.iter().all(|cmd| {
        if let Command::Simple(simple_cmd) = cmd {
            if let Word::Literal(name, _) = &simple_cmd.name {
                if let Some(builtin) = get_builtin_commands().get(name.as_str()) {
                    builtin.supports_linebyline
                } else {
                    false // Non-builtin commands can't do line-by-line
                }
            } else {
                false
            }
        } else if matches!(cmd, Command::While(_)) {
            true // While loops can do line-by-line processing
        } else if let Command::Pipeline(nested_pipeline) = cmd {
            // For nested pipelines, recursively check if they support line-by-line processing
            pipeline_supports_linebyline(nested_pipeline)
        } else {
            false // Other command types can't do line-by-line
        }
    });

    if !all_support_linebyline {
        return false;
    }

    // Additional checks for specific cases where streaming doesn't make sense

    // Check if the first command is an output-generating command like 'yes'
    // But allow line-by-line processing if all subsequent commands support it
    if let Some(Command::Simple(first_cmd)) = pipeline.commands.first() {
        if let Word::Literal(name, _) = &first_cmd.name {
            if name == "yes" {
                // For 'yes' command, check if we can use line-by-line processing
                // by limiting the output and processing line by line
                return true; // Allow line-by-line processing for 'yes' command
            }
        }
    }

    // Check if the first command reads from a file (not STDIN)
    if let Some(Command::Simple(first_cmd)) = pipeline.commands.first() {
        if let Word::Literal(name, _) = &first_cmd.name {
            match name.as_str() {
                "echo" => {
                    // echo produces output, it doesn't read from STDIN
                    // So it should use buffered pipeline, not line-by-line
                    return false;
                }
                "grep" => {
                    // Check for grep options that make streaming inappropriate
                    for arg in &first_cmd.args {
                        if let Word::Literal(arg_str, _) = arg {
                            if arg_str == "-l"
                                || arg_str == "-L"
                                || arg_str == "-Z"
                                || arg_str == "-r"
                            {
                                // These options don't make sense in streaming context
                                // -r (recursive) requires file system traversal
                                return false;
                            }
                        }
                    }

                    // Check if grep has a filename argument (not reading from STDIN)
                    if first_cmd.args.len() > 1 {
                        // Look for the last argument that might be a filename
                        if let Some(last_arg) = first_cmd.args.last() {
                            if let Word::Literal(filename, _) = last_arg {
                                // If it's not an option (doesn't start with -), it's likely a filename
                                if !filename.starts_with('-') {
                                    return false;
                                }
                            }
                        }
                    }
                }
                "cat" => {
                    // If cat has arguments, it's reading from files, not STDIN
                    if !first_cmd.args.is_empty() {
                        return false;
                    }
                }
                _ => {}
            }
        }
    }

    true
}

/// Generate generic Perl code for a builtin command that doesn't need special handling
/// This is a fallback for commands that don't have specialized modules
pub fn generate_generic_builtin(
    generator: &mut Generator,
    cmd: &SimpleCommand,
    input_var: &str,
    output_var: &str,
    command_index: &str,
    linebyline: bool,
) -> String {
    let command_name = match &cmd.name {
        Word::Literal(s, _) => s,
        _ => "unknown_command",
    };

    match command_name {
        "grep" => {
            // Print only for standalone grep (no pipeline input/output)
            let should_print = input_var.is_empty() && output_var.is_empty();
            let mut grep_output = crate::generator::commands::grep::generate_grep_command(
                generator,
                cmd,
                input_var,
                command_index,
                should_print,
            );
            // Assign the grep result to output_var if not already done
            if !output_var.is_empty() && !grep_output.contains(&format!("${} =", output_var)) {
                grep_output.push_str(&format!(
                    "${} = $grep_result_{};\n",
                    output_var, command_index
                ));
            }
            grep_output
        }
        "wc" => {
            // Use the new signature that supports output_var
            crate::generator::commands::wc::generate_wc_command_with_output(
                generator,
                cmd,
                input_var,
                command_index,
                output_var,
            )
        }
        "sort" => {
            // Use the new signature that supports output_var
            crate::generator::commands::sort::generate_sort_command_with_output(
                generator,
                cmd,
                input_var,
                command_index,
                output_var,
            )
        }
        "uniq" => {
            // Use the new signature that supports output_var
            crate::generator::commands::uniq::generate_uniq_command_with_output(
                generator,
                cmd,
                input_var,
                command_index,
                output_var,
            )
        }
        "tr" => {
            // Pass the full command_index string and linebyline parameter
            crate::generator::commands::tr::generate_tr_command(
                generator,
                cmd,
                input_var,
                command_index,
                linebyline,
            )
        }
        "xargs" => {
            // Pass the full command_index string
            crate::generator::commands::xargs::generate_xargs_command(
                generator,
                cmd,
                input_var,
                command_index,
            )
        }
        "ls" => {
            // Use the substitution-specific function for backtick commands
            let ls_output =
                crate::generator::commands::ls::generate_ls_for_substitution(generator, cmd);
            if !output_var.is_empty() {
                // Assign the ls output to the output variable
                format!("${} = {};\n", output_var, ls_output)
            } else {
                ls_output
            }
        }
        // Echo is handled in simple_commands.rs, so use generic fallback
        // "echo" => { ... },
        "echo" => {
            // Use the echo command generator from echo.rs
            crate::generator::commands::echo::generate_echo_command(
                generator, cmd, input_var, output_var,
            )
        }
        "printf" => {
            // Parse command_index to get the numeric part for printf
            let index_num = command_index
                .split('_')
                .next()
                .unwrap_or("0")
                .parse::<usize>()
                .unwrap_or(0);
            let output_var_option = if output_var.is_empty() {
                None
            } else {
                Some(output_var)
            };
            crate::generator::commands::printf::generate_printf_command(
                generator,
                cmd,
                input_var,
                index_num,
                output_var_option,
            )
        }
        "cat" => {
            if cmd.args.is_empty() {
                if output_var.is_empty() {
                    if input_var.is_empty() {
                        "print do { local $INPUT_RECORD_SEPARATOR = undef; <STDIN> };\n".to_string()
                    } else {
                        format!("print ${};\n", input_var)
                    }
                } else if !input_var.is_empty() && input_var == output_var {
                    String::new()
                } else if input_var.is_empty() {
                    format!(
                        "${} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <STDIN> }};\n",
                        output_var
                    )
                } else {
                    format!("${} = ${};\n", output_var, input_var)
                }
            } else {
                crate::generator::commands::cat::generate_cat_command(
                    generator,
                    cmd,
                    &[],
                    output_var,
                )
            }
        }
        "find" => {
            // Use the substitution-specific function for pipeline commands
            let find_output = crate::generator::commands::find::generate_find_for_substitution(
                generator, cmd, "",
            );
            if !output_var.is_empty() {
                // Assign the find output to the output variable
                format!("${} = {};\n", output_var, find_output)
            } else {
                find_output
            }
        }
        "sed" => {
            // For now, use the existing signature but we should standardize this
            // Parse command_index to get the numeric part for sed
            let index_num = command_index
                .split('_')
                .next()
                .unwrap_or("0")
                .parse::<usize>()
                .unwrap_or(0);
            crate::generator::commands::sed::generate_sed_command(
                generator, cmd, input_var, index_num,
            )
        }
        "awk" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::awk::generate_awk_command(generator, cmd, input_var, 0)
        }
        "head" => {
            if input_var.is_empty() {
                let command = Command::Simple(cmd.clone());
                let command_str = generator.generate_command_string_for_system(&command);
                let command_lit = generator.perl_string_literal(&Word::literal(command_str));
                if output_var.is_empty() {
                    format!(
                        "do {{ my $head_cmd = {}; print qx{{$head_cmd}}; }};\n",
                        command_lit
                    )
                } else {
                    format!(
                        "${} = do {{ my $head_cmd = {}; qx{{$head_cmd}}; }};\n",
                        output_var, command_lit
                    )
                }
            } else {
                // For now, use the existing signature but we should standardize this
                // Parse command_index to get the numeric part for head
                let index_num = command_index
                    .split('_')
                    .next()
                    .unwrap_or("0")
                    .parse::<usize>()
                    .unwrap_or(0);
                let mut head_output = crate::generator::commands::head::generate_head_command(
                    generator, cmd, input_var, index_num,
                );
                // Fix the output variable assignment to use output_var instead of input_var
                head_output = head_output.replace(
                    &format!("${} = join(", input_var),
                    &format!("${} = join(", output_var),
                );
                head_output
            }
        }
        "tail" => {
            if input_var.is_empty() {
                let command = Command::Simple(cmd.clone());
                let command_str = generator.generate_command_string_for_system(&command);
                let command_lit = generator.perl_string_literal(&Word::literal(command_str));
                if output_var.is_empty() {
                    format!(
                        "do {{ my $tail_cmd = {}; print qx{{$tail_cmd}}; }};\n",
                        command_lit
                    )
                } else {
                    format!(
                        "${} = do {{ my $tail_cmd = {}; qx{{$tail_cmd}}; }};\n",
                        output_var, command_lit
                    )
                }
            } else {
                crate::generator::commands::tail::generate_tail_command(
                    generator, cmd, input_var, 0,
                )
            }
        }
        "cut" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::cut::generate_cut_command(generator, cmd, input_var, 0)
        }
        "paste" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::paste::generate_paste_command(generator, cmd, &[])
        }
        "comm" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::comm::generate_comm_command(generator, cmd, input_var, &[])
        }
        "diff" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::diff::generate_diff_command(
                generator, cmd, input_var, 0, false,
            )
        }
        "cp" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::cp::generate_cp_command(generator, cmd)
        }
        "mv" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::mv::generate_mv_command(generator, cmd)
        }
        "rm" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::rm::generate_rm_command(generator, cmd)
        }
        "mkdir" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::mkdir::generate_mkdir_command(generator, cmd)
        }
        "touch" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::touch::generate_touch_command(generator, cmd)
        }
        "basename" => {
            // Generate basename command with proper output assignment
            crate::generator::commands::basename::generate_basename_command(
                generator, cmd, input_var, output_var,
            )
        }
        "dirname" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::dirname::generate_dirname_command(generator, cmd, input_var)
        }
        "date" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::date::generate_date_command(generator, cmd)
        }
        "time" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::time::generate_time_command(generator, cmd)
        }
        "sleep" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::sleep::generate_sleep_command(generator, cmd)
        }
        "pwd" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::pwd::generate_pwd_command(generator, cmd)
        }
        "seq" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::seq::generate_seq_command(generator, cmd)
        }
        "which" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::which::generate_which_command(generator, cmd)
        }
        "yes" => {
            // Handle yes command in pipeline context
            crate::generator::commands::yes::generate_yes_command_with_context(
                generator,
                cmd,
                input_var,
                output_var,
                command_index,
            )
        }
        "gzip" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::gzip::generate_gzip_command(generator, cmd, input_var)
        }
        "zcat" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::zcat::generate_zcat_command(generator, cmd)
        }
        "perl" => {
            // Use the specialized Perl command generator
            if input_var.is_empty() {
                // First command in pipeline - use simple command generator
                crate::generator::commands::perl::generate_perl_command(generator, cmd)
            } else {
                // Subsequent command in pipeline - use pipeline command generator
                crate::generator::commands::perl::generate_perl_pipeline_command(
                    generator, cmd, input_var,
                )
            }
        }
        "wget" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::wget::generate_wget_command(generator, cmd)
        }
        "curl" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::curl::generate_curl_command(generator, cmd)
        }
        "kill" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::kill::generate_kill_command(generator, cmd)
        }
        "nohup" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::nohup::generate_nohup_command(generator, cmd)
        }
        "nice" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::nice::generate_nice_command(generator, cmd)
        }
        "sha256sum" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::sha256sum::generate_sha256sum_command(
                generator, cmd, input_var,
            )
        }
        "sha512sum" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::sha512sum::generate_sha512sum_command(
                generator, cmd, input_var,
            )
        }
        "strings" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::strings::generate_strings_command(
                generator, cmd, input_var, output_var,
            )
        }
        "tee" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::tee::generate_tee_command(generator, cmd, input_var)
        }
        "read" => {
            // Handle read command - read from input_var if available, otherwise from STDIN
            if input_var.is_empty() {
                // No input variable, read from STDIN
                format!("my $L = <>;\nchomp $L;\n")
            } else {
                // Read from input variable (pipeline context)
                format!("my $L = ${};\n", input_var)
            }
        }
        "true" => {
            // true command always succeeds (exit status 0)
            if output_var.is_empty() {
                "1;\n".to_string()
            } else {
                format!("1;\n${} = q{};\n", output_var, "")
            }
        }
        "false" => {
            // false command always fails (exit status 1)
            if output_var.is_empty() {
                "exit 1;\n".to_string()
            } else {
                format!("exit 1;\n${} = q{};\n", output_var, "")
            }
        }

        _ => {
            // Fallback for unknown commands - use system call
            generate_system_call_fallback(generator, command_name, cmd, input_var, output_var)
        }
    }
}

/// Generate a system call fallback for unknown commands
fn generate_system_call_fallback(
    generator: &mut Generator,
    command_name: &str,
    cmd: &SimpleCommand,
    input_var: &str,
    output_var: &str,
) -> String {
    // Check if this is a function call with glob patterns
    if generator.declared_functions.contains(command_name) {
        let has_glob_patterns = cmd.args.iter().any(|arg| match arg {
            Word::Literal(s, _) => s.contains('*') || s.contains('?'),
            _ => false,
        });

        if has_glob_patterns {
            // Handle glob pattern expansion for function calls
            let mut output = String::new();
            output.push_str(&generator.indent());
            output.push_str(&format!(
                "for my $file (glob('{}')) {{\n",
                cmd.args[0].as_literal().unwrap_or("*")
            ));
            generator.indent_level += 1;
            output.push_str(&generator.indent());
            output.push_str(&format!("{}($file);\n", command_name));
            generator.indent_level -= 1;
            output.push_str(&generator.indent());
            output.push_str("}\n");
            return output;
        }
    }

    let args: Vec<String> = cmd
        .args
        .iter()
        .filter_map(|arg| match arg {
            Word::Literal(s, _) => Some(s.clone()),
            _ => None,
        })
        .collect();
    let args_str = args.join(" ");

    let (in_var, out_var, err_var, pid_var, _result_var) = generator.get_unique_ipc_vars();
    if input_var.is_empty() {
        // First command in pipeline
        format!("\nmy ({}, {}, {});\nmy {} = open3({}, {}, {}, '{}', {});\nclose {} or croak 'Close failed: $OS_ERROR';\n{} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};\nclose {} or croak 'Close failed: $OS_ERROR';\nwaitpid {}, 0;\n", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, command_name, args_str, in_var, output_var, out_var, out_var, pid_var)
    } else {
        // Subsequent command
        format!("\nmy ({}, {}, {});\nmy {} = open3({}, {}, {}, 'bash', '-c', 'echo \"${}\" | {} {}');\nclose {} or croak 'Close failed: $OS_ERROR';\n{} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};\nclose {} or croak 'Close failed: $OS_ERROR';\nwaitpid {}, 0;\n", in_var, out_var, err_var, pid_var, in_var, out_var, err_var, input_var, command_name, args_str, in_var, output_var, out_var, out_var, pid_var)
    }
}
