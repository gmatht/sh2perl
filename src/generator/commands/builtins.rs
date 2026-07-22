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
    commands.insert("find", BuiltinCommand::new("find", "Find files", false));
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
    // These builtins currently don't have a safe line-by-line streaming
    // implementation in the pipeline generator. Mark them as not supporting
    // line-by-line processing so the buffered pipeline path is chosen
    // (which uses the builtin generator that returns properly-assigned
    // expressions).
    commands.insert(
        "sha256sum",
        BuiltinCommand::new("sha256sum", "Compute SHA256 checksums", false),
    );
    commands.insert(
        "sha512sum",
        BuiltinCommand::new("sha512sum", "Compute SHA512 checksums", false),
    );
    commands.insert(
        "strings",
        // strings can be safely applied line-by-line: printable sequences
        // (ASCII range \x20-\x7E) do not span newlines, so extracting
        // substrings from each input line produces equivalent results and
        // allows streaming pipeline generation. Marking this true fixes
        // cases like `cat file | strings` where the buffered fallback
        // previously emitted a naive length-based filter.
        BuiltinCommand::new("strings", "Extract printable strings", true),
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
    commands.insert(
        "let",
        BuiltinCommand::new("let", "Evaluate arithmetic expressions", false),
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
    let strings_uses_filename = |cmd: &SimpleCommand| -> bool {
        let mut i = 0;
        while i < cmd.args.len() {
            if let Word::Literal(arg_str, _) = &cmd.args[i] {
                if arg_str == "-n" {
                    if i + 1 < cmd.args.len() {
                        if let Word::Literal(next, _) = &cmd.args[i + 1] {
                            if next.parse::<usize>().is_ok() {
                                i += 1;
                            }
                        }
                    }
                } else if arg_str.starts_with("-n") {
                    // Attached form like `-n5` is still an option.
                } else if !arg_str.starts_with('-') {
                    return true;
                }
            }
            i += 1;
        }
        false
    };

    // First check if all commands support line-by-line processing
    let all_support_linebyline = pipeline.commands.iter().all(|cmd| {
        if let Command::Simple(simple_cmd) = cmd {
            if let Word::Literal(name, _) = &simple_cmd.name {
                if name == "strings" && strings_uses_filename(simple_cmd) {
                    false
                } else if let Some(builtin) = get_builtin_commands().get(name.as_str()) {
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
                "strings" => {
                    // If strings has a filename argument, it reads from the file,
                    // not from STDIN, so streaming (line-by-line) is inappropriate.
                    let has_filename = first_cmd.args.iter().any(|arg| {
                        if let Word::Literal(s, _) = arg {
                            !s.starts_with('-')
                        } else {
                            true // non-literal args are treated as filenames
                        }
                    });
                    if has_filename {
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
            // Check if grep is in quiet mode (-q / --quiet / --silent)
            let is_quiet = cmd.args.iter().any(|a| {
                matches!(a, Word::Literal(s, _) if s == "-q" || s == "--quiet" || s == "--silent"
                    || s.starts_with('-') && !s.starts_with("--") && s.contains('q'))
            });
            // Assign the grep result to output_var if not already done; suppress for quiet mode
            if !output_var.is_empty()
                && !is_quiet
                && !grep_output.contains(&format!("${} =", output_var))
            {
                grep_output.push_str(&format!(
                    "${} = $grep_result_{};\n",
                    output_var, command_index
                ));
            } else if !output_var.is_empty() && is_quiet {
                // Quiet mode: clear the output so the pipeline wrapper won't print anything
                grep_output.push_str(&format!("${} = q{{}};\n", output_var));
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
                false,
            )
        }
        "cat" => {
            if cmd.args.is_empty() {
                if output_var.is_empty() {
                    if input_var.is_empty() {
                        "my $cat_stdin = do { local $INPUT_RECORD_SEPARATOR = undef; <STDIN> };\nprint $cat_stdin;\n".to_string()
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
                // If any argument explicitly contains a pipe token, treat this as
                // a shell pipeline invocation rather than as filenames to open.
                let has_pipe = cmd.args.iter().any(|arg| match arg {
                    Word::Literal(s, _) => s == "|",
                    Word::StringInterpolation(interp, _) => {
                        interp.parts.len() == 1
                            && matches!(
                                &interp.parts[0],
                                StringPart::Literal(t) if t == "|"
                            )
                    }
                    _ => false,
                });

                if has_pipe {
                    // Generate the full command string and use qx{} to execute the
                    // pipeline, matching how head/tail handle external commands.
                    let command = Command::Simple(cmd.clone());
                    let command_str = generator.generate_command_string_for_system(&command);
                    // This literal will be used as the qx{} operand to execute the
                    // pipeline at runtime. Emit a non-interpolating Perl literal so
                    // embedded shell $-sequences and escape sequences are preserved.
                    let command_lit =
                        generator.perl_string_literal_no_interp(&Word::literal(command_str));
                    if output_var.is_empty() {
                        // Return the command output as the last expression so callers
                        // (including the purify wrapper) can decide how to print it.
                        format!("do {{ my $cat_cmd = {}; qx{{$cat_cmd}}; }};\n", command_lit)
                    } else {
                        format!(
                            "${} = do {{ my $cat_cmd = {}; qx{{$cat_cmd}}; }};\n",
                            output_var, command_lit
                        )
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
                // When there is no pipeline input (head reads from STDIN/files),
                // use native Perl instead of qx{head} to avoid QX violations
                // and to support streaming input from FIFOs (process substitution).
                // Parse head options to determine number of lines.
                let num_lines = crate::generator::commands::head::get_head_num_lines(cmd);
                if output_var.is_empty() {
                    format!(
                        "do {{ my $__head_count = {}; while (<STDIN>) {{ print $_; last if --$__head_count <= 0; }} }};\n",
                        num_lines
                    )
                } else {
                    format!(
                        "do {{ my $__head_count = {}; my $__head_result = q{{}}; while (<STDIN>) {{ $__head_result .= $_; last if --$__head_count <= 0; }} ${} = $__head_result; }};\n",
                        num_lines, output_var
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
                // Emit non-interpolating literal for qx{} usage
                let command_lit =
                    generator.perl_string_literal_no_interp(&Word::literal(command_str));
                if output_var.is_empty() {
                    // Return the tail output rather than printing it so callers
                    // can choose how to consume the value.
                    format!(
                        "do {{ my $tail_cmd = {}; qx{{$tail_cmd}}; }};\n",
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
            crate::generator::commands::cut::generate_cut_command_with_output(generator, cmd, input_var, 0, output_var)
        }
        "paste" => {
            // For now, use the existing signature but we should standardize this
            let paste_output =
                crate::generator::commands::paste::generate_paste_command(generator, cmd, &[]);
            if output_var.is_empty() {
                // Standalone paste command: capture the do-block result and print it.
                let paste_result_var = format!("paste_result_{}", generator.get_unique_id());
                format!("my ${} = {};\nprint ${};\n", paste_result_var, paste_output, paste_result_var)
            } else {
                format!("${} = {};\n", output_var, paste_output)
            }
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
            // Keep the implementation centralized in sleep.rs.
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
            let sha_code = crate::generator::commands::sha256sum::generate_sha256sum_command(
                generator, cmd, input_var,
            );
            // If an output_var was requested, ensure the generated snippet
            // assigns into that variable. Many callers (pipeline code) pass
            // a bare output_var like "output_0" expecting the builtin to
            // populate it. The sha generators typically return expression
            // snippets; wrap them into an assignment when needed.
            if !output_var.is_empty() {
                // Trim trailing semicolons to avoid double-termination
                let trimmed = sha_code.trim_end_matches('\n').trim_end_matches(';');
                format!("${} = {};;\n", output_var, trimmed)
            } else {
                sha_code
            }
        }
        "sha512sum" => {
            // For now, use the existing signature but we should standardize this
            let sha_code = crate::generator::commands::sha512sum::generate_sha512sum_command(
                generator, cmd, input_var,
            );
            if !output_var.is_empty() {
                let trimmed = sha_code.trim_end_matches('\n').trim_end_matches(';');
                format!("${} = {};;\n", output_var, trimmed)
            } else {
                sha_code
            }
        }
        "strings" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::strings::generate_strings_command(
                generator, cmd, input_var, output_var,
            )
        }
        "tee" => {
            // For now, use the existing signature but we should standardize this
            crate::generator::commands::tee::generate_tee_command(
                generator, cmd, input_var, output_var,
            )
        }
        "read" => {
            // Handle read command - read from input_var if available, otherwise from STDIN
            // Extract variable names from cmd.args (skip flags like -r, -p, -n, -t, -d, -s, -u, -a)
            let vars: Vec<String> = cmd.args.iter().filter_map(|arg| {
                if let Word::Literal(s, _) = arg {
                    if !s.starts_with('-') {
                        return Some(s.clone());
                    }
                }
                None
            }).collect();
            
            if input_var.is_empty() {
                // No input variable, read from STDIN
                if let Some(var_name) = vars.first() {
                    // Assign directly to the named variable without 'my' so the value is
                    // visible in the enclosing scope (e.g. inside a `do` block inside a
                    // while(1) loop). The caller must ensure the variable is already declared.
                    format!(
                        "${} = <>;\nchomp ${};\n$CHILD_ERROR = defined(${}) ? 0 : 1;\n",
                        var_name, var_name, var_name
                    )
                } else {
                    // No variable name given, use $L as fallback
                    format!("my $L = <>;\nchomp $L;\n$CHILD_ERROR = defined($L) ? 0 : 1;\n")
                }
            } else {
                // Read from input variable (pipeline context)
                if let Some(var_name) = vars.first() {
                    format!("${} = ${};\n$CHILD_ERROR = 0;\n", var_name, input_var)
                } else {
                    format!("my $L = ${};\n", input_var)
                }
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
        "whoami" => {
            // whoami - print effective user name
            if output_var.is_empty() {
                "do { my $whoami_user = (getpwuid($<))[0]; print \"$whoami_user\\n\"; $CHILD_ERROR = 0; };\n".to_string()
            } else {
                format!("do {{ my $whoami_user = (getpwuid($<))[0]; ${} = $whoami_user . \"\\n\"; $CHILD_ERROR = 0; }};\n", output_var)
            }
        }
        "uname" => {
            // uname - print system information
            let mut has_flags = false;
            let mut flag_a = false;
            let mut flag_s = false;
            let mut flag_n = false;
            let mut flag_r = false;
            let mut flag_v = false;
            let mut flag_m = false;
            for arg in &cmd.args {
                if let Word::Literal(s, _) = arg {
                    if s.starts_with('-') {
                        has_flags = true;
                        if s.contains('a') { flag_a = true; }
                        if s.contains('s') { flag_s = true; }
                        if s.contains('n') { flag_n = true; }
                        if s.contains('r') { flag_r = true; }
                        if s.contains('v') { flag_v = true; }
                        if s.contains('m') { flag_m = true; }
                    }
                }
            }
            if !has_flags || flag_s { flag_s = true; }
            if flag_a { flag_s = true; flag_n = true; flag_r = true; flag_v = true; flag_m = true; }
            if output_var.is_empty() {
                let mut code = "do { use POSIX qw(uname); my ($__sys, $__node, $__rel, $__ver, $__mach) = POSIX::uname(); my @__parts; ".to_string();
                if flag_s { code.push_str("push @__parts, $__sys; "); }
                if flag_n { code.push_str("push @__parts, $__node; "); }
                if flag_r { code.push_str("push @__parts, $__rel; "); }
                if flag_v { code.push_str("push @__parts, $__ver; "); }
                if flag_m { code.push_str("push @__parts, $__mach; "); }
                code.push_str("print join(\" \", @__parts) . \"\\n\"; $CHILD_ERROR = 0; };\n");
                code
            } else {
                let mut code = format!("do {{ use POSIX qw(uname); my ($__sys, $__node, $__rel, $__ver, $__mach) = POSIX::uname(); my @__parts; ");
                if flag_s { code.push_str("push @__parts, $__sys; "); }
                if flag_n { code.push_str("push @__parts, $__node; "); }
                if flag_r { code.push_str("push @__parts, $__rel; "); }
                if flag_v { code.push_str("push @__parts, $__ver; "); }
                if flag_m { code.push_str("push @__parts, $__mach; "); }
                code.push_str(&format!("${} = join(\" \", @__parts) . \"\\n\"; $CHILD_ERROR = 0; }};\n", output_var));
                code
            }
        }
        "hostname" => {
            // hostname - print system hostname
            if output_var.is_empty() {
                "do { use POSIX qw(uname); my ($__sys, $__node, $__rel, $__ver, $__mach) = POSIX::uname(); print \"$__node\\n\"; $CHILD_ERROR = 0; };\n".to_string()
            } else {
                format!("do {{ use POSIX qw(uname); my ($__sys, $__node, $__rel, $__ver, $__mach) = POSIX::uname(); ${} = $__node . \"\\n\"; $CHILD_ERROR = 0; }};\n", output_var)
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
    // Guard: if command_name looks like an argument (starts with --, or is
    // a value assignment like --flag="value", or contains spaces which
    // indicate it was a positional argument incorrectly parsed as a command),
    // treat it as a no-op. This can happen when bash backslash continuations
    // are not handled by the parser, causing arguments on continuation lines
    // to be parsed as separate commands.
    if command_name.starts_with("--") || command_name.contains('=') || command_name.contains(' ') {
        return format!("$CHILD_ERROR = 0;\n");
    }

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

    // Preserve original string literal semantics when emitting Perl code by
    // using generator.perl_string_literal for each literal argument. This
    // ensures that arguments which originally used double-quote style (and
    // therefore require interpolation at runtime) remain double-quoted in the
    // generated Perl so interpolation happens when the purified script runs.
    let args_perl: Vec<String> = cmd
        .args
        .iter()
        .filter_map(|arg| match arg {
            Word::Literal(_, _) => Some(generator.perl_string_literal(arg)),
            _ => None,
        })
        .collect();
    let args_str = args_perl.join(", ");

    let (in_var, out_var, err_var, pid_var, _result_var) = generator.get_unique_ipc_vars();
    if input_var.is_empty() {
        // First command in pipeline - pass program name and args directly to open3
        format!(
            "\nmy ({}, {}, {});\nmy {} = open3({}, {}, {}, '{}', {});\nclose {} or croak 'Close failed: $OS_ERROR';\n${} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};\nclose {} or croak 'Close failed: $OS_ERROR';\nwaitpid {}, 0;\n",
            in_var, out_var, err_var, pid_var, in_var, out_var, err_var, command_name, args_str, in_var, output_var, out_var, out_var, pid_var
        )
    } else {
        // Subsequent command - build a full shell command string and pass it as
        // the argument to bash -c so pipelines and quoting are preserved.
        let command = Command::Simple(cmd.clone());
        let command_str = generator.generate_command_string_for_system(&command);
        // Use double quotes so Perl interpolates $input_var into the bash command.
        // This correctly passes the previous pipeline stage's output to this command.
        // Escape special characters in the command string for use in double quotes.
        let escaped_cmd = command_str
            .replace("\\", "\\\\")
            .replace("\"", "\\\"")
            .replace("\n", "\\n");
        let command_lit = format!("\"echo \\\"${}\\\" | {}\"", input_var, escaped_cmd);
        format!(
            "\nmy ({}, {}, {});\nmy {} = open3({}, {}, {}, 'bash', '-c', {});\nclose {} or croak 'Close failed: $OS_ERROR';\n${} = do {{ local $INPUT_RECORD_SEPARATOR = undef; <{}> }};\nclose {} or croak 'Close failed: $OS_ERROR';\nwaitpid {}, 0;\n",
            in_var, out_var, err_var, pid_var, in_var, out_var, err_var, command_lit, in_var, output_var, out_var, out_var, pid_var
        )
    }
}
