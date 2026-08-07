pub mod bc_native;
pub mod cache;
pub mod cli_commands;
pub mod execution;
pub mod help;
pub mod testing;
pub mod timeout_manager;
pub mod utils;

// WASI (wasm32-wasip1) CLI-layer ABI — the full command-line processing
// (main_with_args) as a library call for embedders; see wasi_api.rs.
#[cfg(all(target_os = "wasi", feature = "wasi-cli"))]
pub mod wasi_api;

use std::env;
use std::fs;
use std::io::Read;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

// Use the debug module for controlling DEBUG output
use debashl::debug::set_debug_enabled;
use debashl::{shared_utils::SharedUtils, Generator, Parser};

// Global flag for --no-magic-numbers
static NO_MAGIC_NUMBERS: AtomicBool = AtomicBool::new(false);

// `--argv0-source <name>`: the "source-name" $0 semantic. When set, the
// translated program identifies as the ORIGINAL bash file (<name>) instead
// of reporting its own invocation path (argv0 pass-through, the default).
// Perl bakes `$0 = '<name>'`; --estree emits a leading `sh2.argv0 = …`.
// This is the semantic a translation PRODUCT wants (the JS shell executing
// foo.sh should say "foo.sh", not the temp JS file name); pass-through is
// what a faithful POSIX port wants. See harness/argv0-tests/README.md.
static ARGV0_SOURCE: Mutex<Option<String>> = Mutex::new(None);

/// The `--argv0-source` value, if set.
pub fn argv0_source() -> Option<String> {
    ARGV0_SOURCE.lock().unwrap().clone()
}

// Virtual stdin: wasm/JS embedders (node:wasi has no filesystem preopens)
// feed file content through `debashc_cli_run_with_input`; the CLI's `-`
// filename convention (file --estree -, file --perl -, file -) reads this
// when set, else real fd 0. The native CLI never sets it.
static VIRTUAL_STDIN: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// Set the content `-` resolves to (wasm input injection).
pub fn set_virtual_stdin(bytes: Vec<u8>) {
    *VIRTUAL_STDIN.lock().unwrap() = Some(bytes);
}

/// Clear the virtual stdin override.
pub fn clear_virtual_stdin() {
    *VIRTUAL_STDIN.lock().unwrap() = None;
}

/// Run `f` with the current virtual stdin (if any).
pub(crate) fn with_virtual_stdin<T>(f: impl FnOnce(Option<&[u8]>) -> T) -> T {
    let g = VIRTUAL_STDIN.lock().unwrap();
    f(g.as_deref())
}

// Import from our new modules
use crate::cli_commands::{
    export_mir, export_shir, interactive_mode, lex_input, parse_backticks_to_perl, parse_file,
    parse_file_to_estree, parse_file_to_perl, parse_file_to_shir, export_shir_raw, parse_shir_json_to_estree, parse_shir_json_to_perl, parse_file_to_estree_raw, parse_input, parse_system_to_perl,
    parse_to_perl,
    parse_to_perl_inline, parse_to_perl_with_opts,
    run_generated,
};
use crate::help::show_help;
use crate::testing::{
    find_uses_of_system, test_all_examples, test_all_examples_next_fail_unlimited,
    test_file_equivalence, AstFormatOptions,
};
use crate::utils::generate_unified_diff;

fn fix_command_substitution_placeholders(mut code: String) -> String {
    // Fix the specific case of wc -c command substitution that generates $(...) placeholder
    // This is a workaround for the parsing issue with wc -c < "$file" command substitution
    code = code.replace("$(...)", "-s $file");
    code
}

/// Source-name $0 semantic (`--argv0-source <name>`): bake `$0 = '<name>'`
/// into the generated Perl so the translated program identifies as the
/// original bash file, whatever it is invoked as. Default (flag absent) =
/// argv0 pass-through — the harness supplies argv0 at run time.
fn apply_argv0_source(gen: &mut Generator) {
    if let Some(name) = argv0_source() {
        gen.set_original_script_name(name);
    }
}

pub fn main_with_args(args: Vec<String>) {
    let program_name = &args[0];

    if args.len() < 2 {
        show_help(program_name);
        return;
    }

    let command = &args[1];

    if command == "--help" || command == "-h" {
        show_help(&args[0]);
        return;
    }

    // Check for debug control flags early
    if command == "--debug" {
        set_debug_enabled(true);
        // Process remaining arguments as a command
        if args.len() > 2 {
            let remaining_args = &args[2..];
            let new_args = vec![args[0].clone()]
                .into_iter()
                .chain(remaining_args.iter().cloned())
                .collect::<Vec<String>>();
            return main_with_args(new_args);
        }
        return;
    } else if command == "--no-debug" {
        set_debug_enabled(false);
        // Process remaining arguments as a command
        if args.len() > 2 {
            let remaining_args = &args[2..];
            let new_args = vec![args[0].clone()]
                .into_iter()
                .chain(remaining_args.iter().cloned())
                .collect::<Vec<String>>();
            return main_with_args(new_args);
        }
        return;
    } else if command == "--no-magic-numbers" {
        NO_MAGIC_NUMBERS.store(true, Ordering::SeqCst);
        // Process remaining arguments as a command
        if args.len() > 2 {
            let remaining_args = &args[2..];
            let new_args = vec![args[0].clone()]
                .into_iter()
                .chain(remaining_args.iter().cloned())
                .collect::<Vec<String>>();
            return main_with_args(new_args);
        }
        return;
    } else if command == "--argv0-source" {
        if args.len() < 3 {
            println!("Error: --argv0-source requires a name");
            return;
        }
        *ARGV0_SOURCE.lock().unwrap() = Some(args[2].clone());
        // Process remaining arguments as a command (flag + value stripped)
        if args.len() > 3 {
            let remaining_args = &args[3..];
            let new_args = vec![args[0].clone()]
                .into_iter()
                .chain(remaining_args.iter().cloned())
                .collect::<Vec<String>>();
            return main_with_args(new_args);
        }
        return;
    } else if command == "--next-fail" {
        set_debug_enabled(false);
    } else if command == "--freeze" {
        crate::timeout_manager::freeze_execution();
        println!("Execution frozen for debugging. Use --unfreeze to continue.");
        return;
    } else if command == "--unfreeze" {
        crate::timeout_manager::unfreeze_execution();
        println!("Execution unfrozen, continuing...");
        return;
    } else if command == "--timeout-config" {
        if args.len() < 3 {
            println!(
                "Usage: {} --timeout-config <fast|normal|slow|debug>",
                program_name
            );
            return;
        }
        let config_type = &args[2];
        let manager = crate::timeout_manager::get_timeout_manager();
        let mut manager = manager.lock().unwrap();

        match config_type.as_str() {
            "fast" => {
                *manager = crate::timeout_manager::TimeoutManager::with_config(
                    crate::timeout_manager::TimeoutManager::fast_test_config(),
                );
                println!("Timeout configuration set to FAST mode");
            }
            "normal" => {
                *manager = crate::timeout_manager::TimeoutManager::new();
                println!("Timeout configuration set to NORMAL mode");
            }
            "slow" => {
                *manager = crate::timeout_manager::TimeoutManager::with_config(
                    crate::timeout_manager::TimeoutManager::slow_test_config(),
                );
                println!("Timeout configuration set to SLOW mode");
            }
            "debug" => {
                *manager = crate::timeout_manager::TimeoutManager::with_config(
                    crate::timeout_manager::TimeoutManager::debug_config(),
                );
                println!("Timeout configuration set to DEBUG mode");
            }
            _ => {
                println!("Invalid timeout configuration. Use: fast, normal, slow, or debug");
                return;
            }
        }
        return;
    }

    // Parse AST formatting options and input/output options
    let mut ast_options = AstFormatOptions::default();
    let mut input_file: Option<String> = None;
    let mut output_file: Option<String> = None;
    let _optimize_mir = false;
    let mut enable_perl_critic = false;
    let mut _perl_critic_only = false;
    let mut use_function_signatures = true; // Default to modern function signatures
    let mut i = 2;

    // Special case: if the first argument is -i or -o, start parsing from index 1
    if command == "-i" || command == "-o" {
        i = 1;
    }

    while i < args.len() {
        match args[i].as_str() {
            "--debug" => {
                set_debug_enabled(true);
            }
            "--no-debug" => {
                set_debug_enabled(false);
            }
            "--ast-pretty" => {
                ast_options.compact = false;
                ast_options.indent = true;
                ast_options.newlines = true;
            }
            "--ast-compact" => {
                ast_options.compact = true;
                ast_options.indent = false;
                ast_options.newlines = false;
            }
            "--ast-indent" => {
                ast_options.indent = true;
            }
            "--ast-no-indent" => {
                ast_options.indent = false;
            }
            "--ast-newlines" => {
                ast_options.newlines = true;
            }
            "--ast-no-newlines" => {
                ast_options.newlines = false;
            }
            "--perl-critic" => {
                enable_perl_critic = true;
            }
            "--perl-critic-only" => {
                _perl_critic_only = true;
            }
            "--no-function-signatures" => {
                use_function_signatures = false;
                eprintln!("DEBUG: --no-function-signatures option detected, setting use_function_signatures = false");
            }
            "--function-signatures" => {
                use_function_signatures = true;
                eprintln!("DEBUG: --function-signatures option detected, setting use_function_signatures = true");
            }
            "-i" => {
                if i + 1 < args.len() {
                    input_file = Some(args[i + 1].to_string());
                    i += 1; // Skip the next argument since it's the filename
                } else {
                    println!("Error: -i requires a filename");
                    return;
                }
            }
            "-o" => {
                if i + 1 < args.len() {
                    output_file = Some(args[i + 1].to_string());
                    i += 1; // Skip the next argument since it's the filename
                } else {
                    println!("Error: -o requires a filename");
                    return;
                }
            }

            _ => {
                // This might be a filename or other argument
                break;
            }
        }
        i += 1;
    }

    let command = &args[1];

    // Special case: if the first argument is -i or -o, treat it as input/output processing
    if command == "-i" || command == "-o" {
        if let Some(input_filename) = &input_file {
            // Always treat as input file when -i is specified
            match SharedUtils::read_file_lossy(input_filename) {
                Ok(content) => {
                    println!("Processing input file: {}", input_filename);
                    // Parse the shell script
                    let commands = match Parser::new(&content).parse() {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("Parse error: {} — falling back to bash wrapper", e);
                            // Generate a bash wrapper that just runs the shell script
                            let fallback = format!(
                                r##"#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
my $output         = q{{}};
our $CHILD_ERROR;

$main_exit_code = system('bash', '{}') >> 8;

exit $main_exit_code;
"##,
                                input_filename
                            );
                            // Handle output file option
                            if let Some(output_filename) = &output_file {
                                match SharedUtils::write_utf8_file(output_filename, &fallback) {
                                    Ok(_) => println!(
                                        "Generated bash wrapper written to: {} (UTF-8 encoded)",
                                        output_filename
                                    ),
                                    Err(e) => {
                                        println!("Error writing to output file {}: {}", output_filename, e)
                                    }
                                }
                            } else {
                                println!("Generated bash wrapper:");
                                println!("{}", fallback);
                            }
                            return;
                        }
                    };

                    // Generate Perl code
                    let mut gen = Generator::new();
                    gen.use_function_signatures = use_function_signatures;
                    apply_argv0_source(&mut gen);
                    let mut code = gen.generate(&commands);

                    // Post-process to fix command substitution placeholders
                    code = fix_command_substitution_placeholders(code);

                    // Handle output file option
                    if let Some(output_filename) = &output_file {
                        // Write to output file with UTF-8 encoding
                        match SharedUtils::write_utf8_file(output_filename, &code) {
                            Ok(_) => println!(
                                "Generated Perl code written to: {} (UTF-8 encoded)",
                                output_filename
                            ),
                            Err(e) => {
                                println!("Error writing to output file {}: {}", output_filename, e)
                            }
                        }
                    } else {
                        // Show generated code and run it
                        println!("Generated Perl code:");
                        println!("{}", code);
                        println!("\n--- Running generated Perl code ---");
                        let tmp = format!("__tmp_run_{}.pl", std::process::id());
                        if SharedUtils::write_utf8_file(&tmp, &code).is_ok() {
                            let mut cmd = std::process::Command::new("perl");
                            cmd.arg(&tmp);
                            // Run Perl from the examples directory to match the file path adjustments
                            let examples_dir =
                                std::env::current_dir().unwrap_or_default().join("examples");
                            cmd.current_dir(&examples_dir);
                            let _ = cmd.status();
                            let _ = fs::remove_file(&tmp);
                        }
                    }
                }
                Err(e) => {
                    println!("Error reading input file {}: {}", input_filename, e);
                }
            }
        } else {
            println!("Error: -i option requires an input filename");
            return;
        }
        return;
    }

    match command.as_str() {
        "--test-eq" => {
            test_all_examples();
        }
        "--uses-of-system" => {
            find_uses_of_system();
        }
        "--next-fail" => {
            // Disable DEBUG output for --next-fail mode
            set_debug_enabled(false);

            // Parse optional test prefix, generator list, and AST options after --next-fail
            let mut test_prefix: Option<String> = None;
            let mut generators = Vec::new();
            let mut i = 2;

            // Check if first argument is a test prefix (not a number)
            if i < args.len() {
                let arg = &args[i];
                // If it's not a pure number or has leading zeros, treat it as a prefix
                if arg.parse::<usize>().is_err() || arg.len() > 3 || arg.starts_with('0') {
                    test_prefix = Some(arg.clone());
                    i += 1;
                }
            }

            // Collect generators until we hit an AST option or run out of args
            while i < args.len() {
                match args[i].as_str() {
                    "--ast-pretty" | "--ast-compact" | "--ast-indent" | "--ast-no-indent"
                    | "--ast-newlines" | "--ast-no-newlines" => {
                        // Stop parsing generators, let the AST options parsing continue
                        break;
                    }
                    "--perl-critic" => {
                        // Handle --perl-critic flag
                        enable_perl_critic = true;
                        i += 1;
                        continue;
                    }
                    generator => {
                        // Only perl generator is supported
                        if generator == "perl" {
                            generators.push(generator.to_string());
                        } else {
                            println!(
                                "Warning: Only 'perl' generator is supported, skipping '{}'",
                                generator
                            );
                        }
                    }
                }
                i += 1;
            }

            // If no generators specified, default to perl
            if generators.is_empty() {
                generators = vec!["perl".to_string()];
            }

            test_all_examples_next_fail_unlimited(&generators, test_prefix, enable_perl_critic);
        }
        "--clear-cache" => {
            // Clear the unified command cache
            let cache_file = "command_cache.json";
            if let Err(e) = fs::remove_file(cache_file) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    println!("Error removing cache file: {}", e);
                } else {
                    println!("Cache file not found, nothing to clear.");
                }
            } else {
                println!("Command cache cleared successfully.");
            }
        }
        "--diff" => {
            if args.len() < 4 {
                println!("Error: --diff requires two filenames");
                println!("Usage: {} --diff <file1> <file2>", program_name);
                return;
            }
            let file1 = &args[2];
            let file2 = &args[3];

            // Read both files
            let content1 = match fs::read_to_string(file1) {
                Ok(c) => c,
                Err(e) => {
                    println!("Error reading {}: {}", file1, e);
                    return;
                }
            };

            let content2 = match fs::read_to_string(file2) {
                Ok(c) => c,
                Err(e) => {
                    println!("Error reading {}: {}", file2, e);
                    return;
                }
            };

            // Generate and display the diff
            println!("Diffing {} and {}:", file1, file2);
            println!(
                "{}",
                generate_unified_diff(&content1, &content2, file1, file2)
            );
        }
        "lex" => {
            if args.len() < 3 {
                println!("Error: lex command requires input");
                return;
            }
            let input = &args[2];
            // Check if input looks like a filename (contains .sh or doesn't contain spaces)
            if input.contains(".sh") || !input.contains(' ') {
                // Try to read as file first
                match fs::read_to_string(input) {
                    Ok(content) => {
                        lex_input(&content);
                    }
                    Err(_) => {
                        // If file read fails, treat as direct input
                        lex_input(input);
                    }
                }
            } else {
                lex_input(input);
            }
        }
        "parse" | "--ast" => {
            if args.len() < 3 {
                println!("Error: parse command requires input");
                return;
            }
            if args.len() >= 3 && args[2] == "--perl" {
                if args.len() < 4 {
                    println!("Error: parse --perl command requires input");
                    return;
                }
                let input = &args[3];
                if NO_MAGIC_NUMBERS.load(Ordering::SeqCst) {
                    parse_to_perl_with_opts(input, Some(true));
                } else {
                    parse_to_perl(input);
                }
            } else if args.len() >= 3 && args[2] == "--inline" {
                if args.len() < 4 {
                    println!("Error: parse --inline command requires input");
                    return;
                }
                let input = &args[3];
                parse_to_perl_inline(input);
            } else if args.len() >= 3 && args[2] == "--system" {
                if args.len() < 4 {
                    println!("Error: parse --system command requires input");
                    return;
                }
                let input = &args[3];
                parse_system_to_perl(input);
            } else if args.len() >= 3 && args[2] == "--backticks" {
                if args.len() < 4 {
                    println!("Error: parse --backticks command requires input");
                    return;
                }
                let input = &args[3];
                parse_backticks_to_perl(input);
            } else if args.len() >= 3 && args[2] == "--run" {
                // parse --run <lang> <input>
                if args.len() < 5 {
                    println!("Error: parse --run <perl> <input>");
                    return;
                }
                let lang = &args[3];
                let input = &args[4];
                if lang == "perl" {
                    run_generated(lang, input);
                } else {
                    println!("Error: Only 'perl' language is supported");
                    return;
                }
            } else {
                let input = &args[2];
                // If looks like a filename or the path exists, treat as file
                if input.ends_with(".sh") || std::path::Path::new(input).exists() {
                    match fs::read_to_string(input) {
                        Ok(content) => parse_input(&content),
                        Err(_) => parse_input(input),
                    }
                } else {
                    parse_input(input);
                }
            }
        }
        "file" => {
            if args.len() < 3 {
                println!("Error: file command requires filename");
                return;
            }
            if args.len() >= 3 && args[2] == "--perl" {
                if args.len() < 4 {
                    println!("Error: file --perl command requires filename");
                    return;
                }
                let filename = &args[3];
                parse_file_to_perl(filename);
            } else if args.len() >= 3 && args[2] == "--test-file" {
                if args.len() < 5 {
                    println!("Error: file --test-file <perl> <filename>");
                    return;
                }
                let lang = &args[3];
                let filename = &args[4];
                if lang == "perl" {
                    let _ = test_file_equivalence(lang, filename);
                } else {
                    println!("Error: Only 'perl' language is supported");
                    return;
                }
            } else if args.len() >= 3 && args[2] == "--run" {
                if args.len() < 5 {
                    println!("Error: file --run <perl> <filename>");
                    return;
                }
                let lang = &args[3];
                let filename = &args[4];
                if lang == "perl" {
                    run_generated(lang, filename);
                } else {
                    println!("Error: Only 'perl' language is supported");
                    return;
                }
            } else if args.len() >= 3 && args[2] == "--estree" {
                if args.len() < 4 {
                    println!("Error: file --estree requires filename");
                    return;
                }
                let filename = &args[3];
                parse_file_to_estree(filename);
            } else if args.len() >= 3 && args[2] == "--estree-raw" {
                if args.len() < 4 {
                    println!("Error: file --estree-raw requires filename");
                    return;
                }
                let filename = &args[3];
                parse_file_to_estree_raw(filename);
            } else if args.len() >= 3 && args[2] == "--c" {
                if args.len() < 4 {
                    println!("Error: file --c requires filename");
                    return;
                }
                let mut output_lineno = false;
                let mut filename = &args[3];
                if filename == "--output-lineno" {
                    if args.len() < 5 {
                        println!("Error: file --c --output-lineno requires filename");
                        return;
                    }
                    output_lineno = true;
                    filename = &args[4];
                }
                let src = std::fs::read_to_string(filename).unwrap_or_else(|e| {
                    eprintln!("Error reading file {}: {}", filename, e);
                    std::process::exit(1);
                });
                match debashl::cfront::c_to_ir(&src) {
                    Ok(mut prog) => {
                        if !output_lineno {
                            prog.stmt_lines.clear();
                        }
                        println!(
                            "{}",
                            debashl::shir_json::shir_to_shir_json(&prog)
                        );
                    }
                    Err(e) => {
                        eprintln!("{}", e);
                        std::process::exit(1);
                    }
                }
            } else if args.len() >= 3 && args[2] == "--shir" {
                if args.len() < 4 {
                    println!("Error: file --shir requires filename");
                    return;
                }
                let filename = &args[3];
                parse_file_to_shir(filename);
            } else if args.len() >= 3 && args[2] == "--shir-raw" {
                if args.len() < 4 {
                    println!("Error: file --shir-raw requires filename");
                    return;
                }
                let filename = &args[3];
                let content = std::fs::read_to_string(filename).unwrap_or_else(|e| { eprintln!("read {}: {}", filename, e); std::process::exit(1); });
                cli_commands::export_shir_raw(&content);
            } else if args.len() >= 3 && args[2] == "--shir-in-perl" {
                if args.len() < 4 {
                    println!("Error: file --shir-in-perl requires filename");
                    return;
                }
                let filename = &args[3];
                parse_shir_json_to_perl(filename);
            } else if args.len() >= 3 && args[2] == "--shir-in-estree" {
                if args.len() < 4 {
                    println!("Error: file --shir-in-estree requires filename");
                    return;
                }
                let filename = &args[3];
                parse_shir_json_to_estree(filename);
            } else if args.len() >= 3 && args[2] == "--perl-critic-only" {
                if args.len() < 4 {
                    println!("Error: file --perl-critic-only requires filename");
                    return;
                }
                let filename = &args[3];
                match fs::read_to_string(filename) {
                    Ok(content) => {
                        cli_commands::parse_perl_critic_only(&content);
                    }
                    Err(e) => {
                        println!("Error reading file {}: {}", filename, e);
                        std::process::exit(1);
                    }
                }
            } else {
                let filename = &args[2];
                parse_file(filename);
            }
        }
        "--test-file" | "test-file" => {
            if args.len() < 4 {
                println!("Error: --test-file <perl> <filename>");
                return;
            }
            let lang = &args[2];
            let filename = &args[3];
            if lang == "perl" {
                if let Err(e) = test_file_equivalence(lang, filename) {
                    eprintln!("FAIL: {}", e);
                    std::process::exit(1);
                }
            } else {
                println!("Error: Only 'perl' language is supported");
                std::process::exit(1);
            }
        }
        "interactive" => {
            interactive_mode();
        }
        "--perl-critic-only" => {
            if args.len() < 3 {
                println!("Error: --perl-critic-only requires input");
                return;
            }
            let input = &args[2];
            // Check if input looks like a filename (contains .sh or doesn't contain spaces)
            if input.contains(".sh") || !input.contains(' ') {
                // Try to read as file first
                match fs::read_to_string(input) {
                    Ok(content) => {
                        cli_commands::parse_perl_critic_only(&content);
                    }
                    Err(_) => {
                        // If file read fails, treat as direct input
                        cli_commands::parse_perl_critic_only(input);
                    }
                }
            } else {
                cli_commands::parse_perl_critic_only(input);
            }
        }
        "c" => {
            // the minimal C frontend: parse a portable-C subset and emit
            // the SAME ShIR JSON contract the shell frontend produces
            // (frontend-c-core-needs.md)
            if args.len() < 3 {
                println!("Error: c command requires input");
                return;
            }
            // --output-lineno: keep the per-statement source line numbers
            // in the JSON (the Perl renderer turns them into ` # line N`
            // end-of-line comments)
            let mut output_lineno = false;
            let mut input = &args[2];
            if input == "--output-lineno" {
                if args.len() < 4 {
                    println!("Error: c --output-lineno requires input");
                    return;
                }
                output_lineno = true;
                input = &args[3];
            }
            let src = if input == "-" {
                let mut s = String::new();
                if let Err(e) = std::io::stdin().read_to_string(&mut s) {
                    eprintln!("stdin: {}", e);
                    std::process::exit(1);
                }
                s
            } else if std::path::Path::new(input).exists() {
                std::fs::read_to_string(input).unwrap_or_else(|e| {
                    eprintln!("Error reading file {}: {}", input, e);
                    std::process::exit(1);
                })
            } else {
                input.to_string()
            };
            match debashl::cfront::c_to_ir(&src) {
                Ok(mut prog) => {
                    if !output_lineno {
                        prog.stmt_lines.clear();
                    }
                    println!(
                        "{}",
                        debashl::shir_json::shir_to_shir_json(&prog)
                    );
                }
                Err(e) => {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
            return;
        }
        "--shir" => {
            if args.len() < 3 {
                println!("Error: --shir command requires input");
                return;
            }
            let input = &args[2];
            // Optional --raw (suppress trailing newline for machine consumers).
            let raw = args.len() >= 4 && args[3] == "--raw";
            // "-" means stdin (plan improvement #2); file-like input tries
            // a file read first and falls back to direct string.
            if input == "-" {
                let mut s = String::new();
                if let Err(e) = std::io::stdin().read_to_string(&mut s) {
                    eprintln!("stdin: {}", e); std::process::exit(1);
                }
                cli_commands::export_shir(&s, raw);
            } else if input.contains(".sh") || !input.contains(' ') {
                match fs::read_to_string(input) {
                    Ok(content) => cli_commands::export_shir(&content, raw),
                    Err(_) => cli_commands::export_shir(input, raw),
                }
            } else {
                cli_commands::export_shir(input, raw);
            }
        }
        "--shir-raw" => {
            if args.len() < 3 { println!("Error: --shir-raw requires input"); return; }
            let input = &args[2];
            if input == "-" {
                let mut s = String::new();
                if let Err(e) = std::io::stdin().read_to_string(&mut s) {
                    eprintln!("stdin: {}", e); std::process::exit(1);
                }
                cli_commands::export_shir_raw(&s);
            } else if input.contains(".sh") || !input.contains(' ') {
                match fs::read_to_string(input) {
                    Ok(c) => cli_commands::export_shir_raw(&c),
                    Err(_) => cli_commands::export_shir_raw(input),
                }
            } else { cli_commands::export_shir_raw(input); }
        }
        "--shir-in-estree" => {
            if args.len() < 3 { println!("Error: --shir-in-estree requires input"); return; }
            let input = &args[2];
            let content = if input == "-" {
                let mut s = String::new();
                if let Err(e) = std::io::stdin().read_to_string(&mut s) {
                    eprintln!("stdin: {}", e); std::process::exit(1);
                }
                Ok(s)
            } else {
                fs::read_to_string(input)
            };
            let content = match content {
                Ok(c) => c,
                Err(_) => { eprintln!("cannot read {}", input); std::process::exit(1); }
            };
            let mut prog = match debashl::shir_json_in::shir_json_to_ir(&content) {
                Ok(p) => p,
                Err(e) => { eprintln!("ShIR JSON ingress: {}", e); std::process::exit(1); }
            };
            debashl::shir_passes::restructure_goto_only(&mut prog);
            match debashl::shir::shir_to_estree_json(&prog) {
                Ok(s) => println!("{}", s),
                Err(e) => { eprintln!("estree: {}", e); std::process::exit(1); }
            }
        }
        "--shir-in-perl" => {
            if args.len() < 3 { println!("Error: --shir-in-perl requires input"); return; }
            let input = &args[2];
            let content = if input == "-" {
                let mut s = String::new();
                if let Err(e) = std::io::stdin().read_to_string(&mut s) {
                    eprintln!("stdin: {}", e); std::process::exit(1);
                }
                Ok(s)
            } else {
                fs::read_to_string(input)
            };
            let content = match content {
                Ok(c) => c,
                Err(_) => { eprintln!("cannot read {}", input); std::process::exit(1); }
            };
            let mut prog = match debashl::shir_json_in::shir_json_to_ir(&content) {
                Ok(p) => p,
                Err(e) => { eprintln!("ShIR JSON ingress: {}", e); std::process::exit(1); }
            };
            debashl::shir_passes::restructure_goto_only(&mut prog);
            print!("{}", debashl::ir::shir_to_perl(&prog));
        }
        "--mir" => {
            if args.len() < 3 {
                println!("Error: --mir command requires input");
                return;
            }

            // Parse --mir specific options
            let mut mir_optimize = false;
            let mut input_index = 2;

            // Check for -O flag
            if args.len() > 3 && args[2] == "-O" {
                mir_optimize = true;
                input_index = 3;
            }

            if input_index >= args.len() {
                println!("Error: --mir command requires input");
                return;
            }

            let input = &args[input_index];
            // Check if input looks like a filename (contains .sh or doesn't contain spaces)
            if input.contains(".sh") || !input.contains(' ') {
                // Try to read as file first
                match fs::read_to_string(input) {
                    Ok(content) => {
                        export_mir(&content, mir_optimize);
                    }
                    Err(_) => {
                        // If file read fails, treat as direct input
                        export_mir(input, mir_optimize);
                    }
                }
            } else {
                export_mir(input, mir_optimize);
            }
        }
        "fail" => {
            // Shorthand for --next-fail
            // Disable DEBUG output for fail mode
            set_debug_enabled(false);

            // Parse optional test prefix, generator list, and AST options after fail
            let mut test_prefix: Option<String> = None;
            let mut generators = Vec::new();
            let mut i = 2;

            // First pass: collect flags and generators
            while i < args.len() {
                match args[i].as_str() {
                    "--ast-pretty" | "--ast-compact" | "--ast-indent" | "--ast-no-indent"
                    | "--ast-newlines" | "--ast-no-newlines" => {
                        // Stop parsing generators, let the AST options parsing continue
                        break;
                    }
                    "--perl-critic" => {
                        // Handle --perl-critic flag
                        enable_perl_critic = true;
                        i += 1;
                        continue;
                    }
                    generator => {
                        // Only perl generator is supported
                        if generator == "perl" {
                            generators.push(generator.to_string());
                        } else {
                            // If it's not a generator, treat it as a test prefix
                            test_prefix = Some(generator.to_string());
                        }
                    }
                }
                i += 1;
            }

            // If no generators specified, default to perl
            if generators.is_empty() {
                generators = vec!["perl".to_string()];
            }

            // Always run all tests (no limits)
            test_all_examples_next_fail_unlimited(&generators, test_prefix, enable_perl_critic);
        }
        _ => {
            // Handle input file option
            if let Some(input_filename) = &input_file {
                // Always treat as input file when -i is specified
                match SharedUtils::read_file_lossy(input_filename) {
                    Ok(content) => {
                        println!("Processing input file: {}", input_filename);
                        // Parse the shell script
                        let commands = match Parser::new(&content).parse() {
                            Ok(c) => c,
                            Err(e) => {
                                eprintln!("Parse error: {}", e);
                                std::process::exit(1);
                            }
                        };

                        // Generate Perl code
                        let mut gen = Generator::new();
                        gen.use_function_signatures = use_function_signatures;
                        apply_argv0_source(&mut gen);
                        let code = gen.generate(&commands);

                        // Handle output file option
                        if let Some(output_filename) = &output_file {
                            // Write to output file with UTF-8 encoding
                            match SharedUtils::write_utf8_file(output_filename, &code) {
                                Ok(_) => println!(
                                    "Generated Perl code written to: {} (UTF-8 encoded)",
                                    output_filename
                                ),
                                Err(e) => println!(
                                    "Error writing to output file {}: {}",
                                    output_filename, e
                                ),
                            }
                        } else {
                            // Show generated code and run it
                            println!("Generated Perl code:");
                            println!("{}", code);
                            println!("\n--- Running generated Perl code ---");
                            let tmp = format!("__tmp_run_{}.pl", std::process::id());
                            if SharedUtils::write_utf8_file(&tmp, &code).is_ok() {
                                let mut cmd = std::process::Command::new("perl");
                                cmd.arg(&tmp);
                                // Run Perl from the examples directory to match the file path adjustments
                                let examples_dir =
                                    std::env::current_dir().unwrap_or_default().join("examples");
                                cmd.current_dir(&examples_dir);
                                let _ = cmd.status();
                                let _ = fs::remove_file(&tmp);
                            }
                        }
                    }
                    Err(e) => {
                        println!("Error reading input file {}: {}", input_filename, e);
                    }
                }
            } else if command.ends_with(".sh") {
                // Run the shell script directly
                match fs::read_to_string(command) {
                    Ok(content) => {
                        println!("Running shell script: {}", command);
                        // Parse and run the shell script
                        let commands = match Parser::new(&content).parse() {
                            Ok(c) => c,
                            Err(e) => {
                                // Fallback: generate a bash wrapper that runs the original script
                                let fallback = format!(
                                    r##"#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
my $output         = q{{}};
our $CHILD_ERROR;

$main_exit_code = system('bash', '{}') >> 8;

exit $main_exit_code;
"##,
                                    command
                                );
                                println!("Generated Perl code:");
                                println!("{}", fallback);
                                println!("\n--- Running generated Perl code ---");
                                return;
                            }
                        };

                        // Generate Perl code
                        let mut gen = Generator::new();
                        gen.use_function_signatures = use_function_signatures;
                        apply_argv0_source(&mut gen);
                        let code = gen.generate(&commands);

                        // Handle output file option
                        if let Some(output_filename) = &output_file {
                            // Write to output file with UTF-8 encoding
                            match SharedUtils::write_utf8_file(output_filename, &code) {
                                Ok(_) => println!(
                                    "Generated Perl code written to: {} (UTF-8 encoded)",
                                    output_filename
                                ),
                                Err(e) => println!(
                                    "Error writing to output file {}: {}",
                                    output_filename, e
                                ),
                            }
                        } else {
                            // Show generated code and run it
                            println!("Generated Perl code:");
                            println!("{}", code);
                            println!("\n--- Running generated Perl code ---");
                            let tmp = format!("__tmp_run_{}.pl", std::process::id());
                            if SharedUtils::write_utf8_file(&tmp, &code).is_ok() {
                                // Time the Perl execution
                                let perl_start = std::time::Instant::now();
                                let mut cmd = std::process::Command::new("perl");
                                cmd.arg(&tmp);
                                // Run Perl from the examples directory to match the file path adjustments
                                let examples_dir =
                                    std::env::current_dir().unwrap_or_default().join("examples");
                                cmd.current_dir(&examples_dir);
                                let perl_output = cmd.output();
                                let perl_duration = perl_start.elapsed();

                                // Time the bash execution
                                let bash_start = std::time::Instant::now();
                                let bash_output = std::process::Command::new("sh")
                                    .arg("-c")
                                    .arg(&content)
                                    .output();
                                let bash_duration = bash_start.elapsed();

                                match (perl_output, bash_output) {
                                    (Ok(perl_out), Ok(bash_out)) => {
                                        let perl_stdout =
                                            String::from_utf8_lossy(&perl_out.stdout).to_string();
                                        let perl_stderr =
                                            String::from_utf8_lossy(&perl_out.stderr).to_string();
                                        let bash_stdout =
                                            String::from_utf8_lossy(&bash_out.stdout).to_string();
                                        let bash_stderr =
                                            String::from_utf8_lossy(&bash_out.stderr).to_string();

                                        // Display Perl output
                                        if !perl_stdout.is_empty() {
                                            print!("{}", perl_stdout);
                                        }
                                        if !perl_stderr.is_empty() {
                                            eprint!("{}", perl_stderr);
                                        }
                                        println!("Exit code: {}", perl_out.status);

                                        // Display timing information
                                        println!("\n{}", "=".repeat(50));
                                        println!("TIMING COMPARISON");
                                        println!("{}", "=".repeat(50));
                                        println!(
                                            "Perl execution time:  {:.4} seconds",
                                            perl_duration.as_secs_f64()
                                        );
                                        println!(
                                            "Bash execution time:  {:.4} seconds",
                                            bash_duration.as_secs_f64()
                                        );

                                        let speedup = if perl_duration.as_secs_f64() > 0.0 {
                                            bash_duration.as_secs_f64()
                                                / perl_duration.as_secs_f64()
                                        } else {
                                            0.0
                                        };

                                        if speedup > 1.0 {
                                            println!("Perl is {:.2}x faster than Bash", speedup);
                                        } else if speedup > 0.0 {
                                            println!(
                                                "Bash is {:.2}x faster than Perl",
                                                1.0 / speedup
                                            );
                                        } else {
                                            println!("Cannot calculate speedup (Perl execution time was 0)");
                                        }

                                        // Display diff output
                                        println!("\n{}", "=".repeat(50));
                                        println!("OUTPUT COMPARISON");
                                        println!("{}", "=".repeat(50));

                                        let stdout_match = perl_stdout.trim() == bash_stdout.trim();
                                        let stderr_match = perl_stderr.trim() == bash_stderr.trim();
                                        let exit_match =
                                            perl_out.status.code() == bash_out.status.code();

                                        if stdout_match && stderr_match && exit_match {
                                            println!("✓ PERFECT MATCH: Perl and Bash outputs are identical!");
                                        } else {
                                            println!("✗ DIFFERENCES FOUND:");

                                            if !stdout_match {
                                                println!("\nSTDOUT DIFFERENCES:");
                                                println!(
                                                    "{}",
                                                    generate_unified_diff(
                                                        &bash_stdout,
                                                        &perl_stdout,
                                                        "bash_stdout",
                                                        "perl_stdout"
                                                    )
                                                );
                                            }

                                            if !stderr_match {
                                                println!("\nSTDERR DIFFERENCES:");
                                                println!(
                                                    "{}",
                                                    generate_unified_diff(
                                                        &bash_stderr,
                                                        &perl_stderr,
                                                        "bash_stderr",
                                                        "perl_stderr"
                                                    )
                                                );
                                            }

                                            if !exit_match {
                                                println!("\nEXIT CODE DIFFERENCES:");
                                                println!(
                                                    "Bash exit code: {:?}",
                                                    bash_out.status.code()
                                                );
                                                println!(
                                                    "Perl exit code: {:?}",
                                                    perl_out.status.code()
                                                );
                                            }
                                        }
                                    }
                                    (Ok(perl_out), Err(bash_err)) => {
                                        // Perl succeeded but bash failed
                                        if !perl_out.stdout.is_empty() {
                                            print!("{}", String::from_utf8_lossy(&perl_out.stdout));
                                        }
                                        if !perl_out.stderr.is_empty() {
                                            eprint!(
                                                "{}",
                                                String::from_utf8_lossy(&perl_out.stderr)
                                            );
                                        }
                                        println!("Exit code: {}", perl_out.status);
                                        println!("\nBash execution failed: {}", bash_err);
                                    }
                                    (Err(perl_err), Ok(bash_out)) => {
                                        // Bash succeeded but Perl failed
                                        println!("Perl execution failed: {}", perl_err);
                                        if !bash_out.stdout.is_empty() {
                                            print!(
                                                "Bash output: {}",
                                                String::from_utf8_lossy(&bash_out.stdout)
                                            );
                                        }
                                        if !bash_out.stderr.is_empty() {
                                            eprint!(
                                                "Bash stderr: {}",
                                                String::from_utf8_lossy(&bash_out.stderr)
                                            );
                                        }
                                        println!("Bash exit code: {}", bash_out.status);
                                    }
                                    (Err(perl_err), Err(bash_err)) => {
                                        // Both failed
                                        println!("Perl execution failed: {}", perl_err);
                                        println!("Bash execution failed: {}", bash_err);
                                    }
                                }

                                // Clean up temporary file
                                let _ = fs::remove_file(tmp);
                            }
                        }
                    }
                    Err(e) => {
                        println!("Error reading file {}: {}", command, e);
                    }
                }
            } else {
                // Parse options for unknown commands
                let mut i = 1;
                let mut actual_command = command.clone();
                while i < args.len() {
                    match args[i].as_str() {
                        "--no-function-signatures" => {
                            use_function_signatures = false;
                            eprintln!("DEBUG: --no-function-signatures option detected, setting use_function_signatures = false");
                        }
                        "--function-signatures" => {
                            use_function_signatures = true;
                            eprintln!("DEBUG: --function-signatures option detected, setting use_function_signatures = true");
                        }
                        _ => {
                            // This might be a filename or other argument
                            if actual_command == *command {
                                actual_command = args[i].clone();
                            }
                            break;
                        }
                    }
                    i += 1;
                }

                // Check if it's a .sh file
                if actual_command.ends_with(".sh") {
                    // Run the shell script directly
                    match fs::read_to_string(&actual_command) {
                        Ok(content) => {
                            println!("Running shell script: {}", actual_command);
                            // Parse and run the shell script
                            let commands = match Parser::new(&content).parse() {
                                Ok(c) => c,
                                Err(e) => {
                                    eprintln!("Parse error: {}", e);
                                    std::process::exit(1);
                                }
                            };

                            // Generate Perl code
                            let mut gen = Generator::new();
                            gen.use_function_signatures = use_function_signatures;
                            apply_argv0_source(&mut gen);
                            let perl_code = gen.generate(&commands);

                            // Write to temporary file and execute
                            let tmp_file = format!("__tmp_run_{}.pl", std::process::id());
                            if SharedUtils::write_utf8_file(&tmp_file, &perl_code).is_ok() {
                                println!("Generated Perl code:");
                                println!("{}", perl_code);
                                println!("\n--- Running generated Perl code ---");

                                // Time the Perl execution
                                let perl_start = std::time::Instant::now();
                                let mut cmd = std::process::Command::new("perl");
                                cmd.arg(&tmp_file);
                                // Run Perl from the examples directory to match the file path adjustments
                                let examples_dir =
                                    std::env::current_dir().unwrap_or_default().join("examples");
                                cmd.current_dir(&examples_dir);
                                let perl_output = cmd.output();
                                let perl_duration = perl_start.elapsed();

                                // Time the bash execution
                                let bash_start = std::time::Instant::now();
                                let bash_output = std::process::Command::new("bash")
                                    .arg(&actual_command)
                                    .output();
                                let bash_duration = bash_start.elapsed();

                                // Clean up temporary file
                                let _ = std::fs::remove_file(tmp_file);

                                match (perl_output, bash_output) {
                                    (Ok(perl_out), Ok(bash_out)) => {
                                        let perl_stdout = String::from_utf8_lossy(&perl_out.stdout);
                                        let perl_stderr = String::from_utf8_lossy(&perl_out.stderr);
                                        let bash_stdout = String::from_utf8_lossy(&bash_out.stdout);
                                        let bash_stderr = String::from_utf8_lossy(&bash_out.stderr);

                                        println!("{}", perl_stdout);
                                        if !perl_stderr.is_empty() {
                                            eprint!("{}", perl_stderr);
                                        }
                                        println!("Exit code: {}", perl_out.status);

                                        println!("\n{}", "=".repeat(50));
                                        println!("TIMING COMPARISON");
                                        println!("{}", "=".repeat(50));
                                        println!(
                                            "Perl execution time:  {:.4} seconds",
                                            perl_duration.as_secs_f64()
                                        );
                                        println!(
                                            "Bash execution time:  {:.4} seconds",
                                            bash_duration.as_secs_f64()
                                        );
                                        if bash_duration.as_secs_f64() > 0.0 {
                                            let speedup = bash_duration.as_secs_f64()
                                                / perl_duration.as_secs_f64();
                                            if speedup > 1.0 {
                                                println!(
                                                    "Bash is {:.2}x faster than Perl",
                                                    speedup
                                                );
                                            } else {
                                                println!(
                                                    "Perl is {:.2}x faster than Bash",
                                                    1.0 / speedup
                                                );
                                            }
                                        }

                                        println!("\n{}", "=".repeat(50));
                                        println!("OUTPUT COMPARISON");
                                        println!("{}", "=".repeat(50));

                                        let stdout_match = perl_stdout == bash_stdout;
                                        let stderr_match = perl_stderr == bash_stderr;
                                        let exit_match =
                                            perl_out.status.code() == bash_out.status.code();

                                        if stdout_match && stderr_match && exit_match {
                                            println!("✓ PERFECT MATCH!");
                                        } else {
                                            println!("✗ DIFFERENCES FOUND:");

                                            if !stdout_match {
                                                println!("\nSTDOUT DIFFERENCES:");
                                                println!(
                                                    "{}",
                                                    generate_unified_diff(
                                                        &bash_stdout,
                                                        &perl_stdout,
                                                        "bash_stdout",
                                                        "perl_stdout"
                                                    )
                                                );
                                            }

                                            if !stderr_match {
                                                println!("\nSTDERR DIFFERENCES:");
                                                println!(
                                                    "{}",
                                                    generate_unified_diff(
                                                        &bash_stderr,
                                                        &perl_stderr,
                                                        "bash_stderr",
                                                        "perl_stderr"
                                                    )
                                                );
                                            }

                                            if !exit_match {
                                                println!("\nEXIT CODE DIFFERENCES:");
                                                println!(
                                                    "Bash exit code: {:?}",
                                                    bash_out.status.code()
                                                );
                                                println!(
                                                    "Perl exit code: {:?}",
                                                    perl_out.status.code()
                                                );
                                            }
                                        }
                                    }
                                    (Ok(perl_out), Err(bash_err)) => {
                                        // Perl succeeded but bash failed
                                        if !perl_out.stdout.is_empty() {
                                            print!("{}", String::from_utf8_lossy(&perl_out.stdout));
                                        }
                                        if !perl_out.stderr.is_empty() {
                                            eprint!(
                                                "{}",
                                                String::from_utf8_lossy(&perl_out.stderr)
                                            );
                                        }
                                        println!("Exit code: {}", perl_out.status);
                                        println!("\nBash execution failed: {}", bash_err);
                                    }
                                    (Err(perl_err), Ok(bash_out)) => {
                                        // Bash succeeded but Perl failed
                                        if !bash_out.stdout.is_empty() {
                                            print!("{}", String::from_utf8_lossy(&bash_out.stdout));
                                        }
                                        if !bash_out.stderr.is_empty() {
                                            eprint!(
                                                "{}",
                                                String::from_utf8_lossy(&bash_out.stderr)
                                            );
                                        }
                                        println!("Exit code: {}", bash_out.status);
                                        println!("\nPerl execution failed: {}", perl_err);
                                    }
                                    (Err(perl_err), Err(bash_err)) => {
                                        println!("Both Perl and Bash execution failed:");
                                        println!("Perl error: {}", perl_err);
                                        println!("Bash error: {}", bash_err);
                                    }
                                }
                            } else {
                                println!("Error writing temporary Perl file");
                            }
                        }
                        Err(e) => {
                            println!("Error reading file {}: {}", actual_command, e);
                        }
                    }
                } else {
                    // Treat unknown commands as shell commands to be executed with timing and diff
                    println!("Executing shell command: {}", actual_command);
                    println!("{}", "=".repeat(50));

                    // Parse the command as shell input
                    match Parser::new(&actual_command).parse() {
                        Ok(commands) => {
                            // Generate Perl code
                            let mut generator = Generator::new();
                            generator.use_function_signatures = use_function_signatures;
                            apply_argv0_source(&mut generator);
                            let perl_code = generator.generate(&commands);

                            // Write to temporary file and execute
                            let tmp_file = "__tmp_direct_exec.pl";
                            if SharedUtils::write_utf8_file(tmp_file, &perl_code).is_ok() {
                                println!("Generated Perl code:");
                                println!("{}", perl_code);
                                println!("\n--- Running generated Perl code ---");

                                // Time the Perl execution
                                let perl_start = std::time::Instant::now();
                                let mut cmd = std::process::Command::new("perl");
                                cmd.arg(tmp_file);
                                // Run Perl from the examples directory to match the file path adjustments
                                let examples_dir =
                                    std::env::current_dir().unwrap_or_default().join("examples");
                                cmd.current_dir(&examples_dir);
                                let perl_output = cmd.output();
                                let perl_duration = perl_start.elapsed();

                                // Time the bash execution
                                let bash_start = std::time::Instant::now();

                                // Remove single quotes from the command if present
                                let bash_command =
                                    if command.starts_with("'") && command.ends_with("'") {
                                        &command[1..command.len() - 1]
                                    } else {
                                        command
                                    };

                                // Try using sh instead of bash for better compatibility
                                let bash_output = std::process::Command::new("sh")
                                    .arg("-c")
                                    .arg(bash_command)
                                    .output();
                                let bash_duration = bash_start.elapsed();

                                match (perl_output, bash_output) {
                                    (Ok(perl_out), Ok(bash_out)) => {
                                        let perl_stdout =
                                            String::from_utf8_lossy(&perl_out.stdout).to_string();
                                        let perl_stderr =
                                            String::from_utf8_lossy(&perl_out.stderr).to_string();
                                        let bash_stdout =
                                            String::from_utf8_lossy(&bash_out.stdout).to_string();
                                        let bash_stderr =
                                            String::from_utf8_lossy(&bash_out.stderr).to_string();

                                        // Display Perl output
                                        if !perl_stdout.is_empty() {
                                            print!("{}", perl_stdout);
                                        }
                                        if !perl_stderr.is_empty() {
                                            eprint!("{}", perl_stderr);
                                        }
                                        println!("Exit code: {}", perl_out.status);

                                        // Display timing information
                                        println!("\n{}", "=".repeat(50));
                                        println!("TIMING COMPARISON");
                                        println!("{}", "=".repeat(50));
                                        println!(
                                            "Perl execution time:  {:.4} seconds",
                                            perl_duration.as_secs_f64()
                                        );
                                        println!(
                                            "Bash execution time:  {:.4} seconds",
                                            bash_duration.as_secs_f64()
                                        );

                                        let speedup = if perl_duration.as_secs_f64() > 0.0 {
                                            bash_duration.as_secs_f64()
                                                / perl_duration.as_secs_f64()
                                        } else {
                                            0.0
                                        };

                                        if speedup > 1.0 {
                                            println!("Perl is {:.2}x faster than Bash", speedup);
                                        } else if speedup > 0.0 {
                                            println!(
                                                "Bash is {:.2}x faster than Perl",
                                                1.0 / speedup
                                            );
                                        } else {
                                            println!("Cannot calculate speedup (Perl execution time was 0)");
                                        }

                                        // Display diff output
                                        println!("\n{}", "=".repeat(50));
                                        println!("OUTPUT COMPARISON");
                                        println!("{}", "=".repeat(50));

                                        let stdout_match = perl_stdout.trim() == bash_stdout.trim();
                                        let stderr_match = perl_stderr.trim() == bash_stderr.trim();
                                        let exit_match =
                                            perl_out.status.code() == bash_out.status.code();

                                        if stdout_match && stderr_match && exit_match {
                                            println!("✓ PERFECT MATCH: Perl and Bash outputs are identical!");
                                        } else {
                                            println!("✗ DIFFERENCES FOUND:");

                                            if !stdout_match {
                                                println!("\nSTDOUT DIFFERENCES:");
                                                println!(
                                                    "{}",
                                                    generate_unified_diff(
                                                        &bash_stdout,
                                                        &perl_stdout,
                                                        "bash_stdout",
                                                        "perl_stdout"
                                                    )
                                                );
                                            }

                                            if !stderr_match {
                                                println!("\nSTDERR DIFFERENCES:");
                                                println!(
                                                    "{}",
                                                    generate_unified_diff(
                                                        &bash_stderr,
                                                        &perl_stderr,
                                                        "bash_stderr",
                                                        "perl_stderr"
                                                    )
                                                );
                                            }

                                            if !exit_match {
                                                println!("\nEXIT CODE DIFFERENCES:");
                                                println!(
                                                    "Bash exit code: {:?}",
                                                    bash_out.status.code()
                                                );
                                                println!(
                                                    "Perl exit code: {:?}",
                                                    perl_out.status.code()
                                                );
                                            }
                                        }
                                    }
                                    (Ok(perl_out), Err(bash_err)) => {
                                        // Perl succeeded but bash failed
                                        if !perl_out.stdout.is_empty() {
                                            print!("{}", String::from_utf8_lossy(&perl_out.stdout));
                                        }
                                        if !perl_out.stderr.is_empty() {
                                            eprint!(
                                                "{}",
                                                String::from_utf8_lossy(&perl_out.stderr)
                                            );
                                        }
                                        println!("Exit code: {}", perl_out.status);
                                        println!("\nBash execution failed: {}", bash_err);
                                    }
                                    (Err(perl_err), Ok(bash_out)) => {
                                        // Bash succeeded but Perl failed
                                        println!("Perl execution failed: {}", perl_err);
                                        if !bash_out.stdout.is_empty() {
                                            print!(
                                                "Bash output: {}",
                                                String::from_utf8_lossy(&bash_out.stdout)
                                            );
                                        }
                                        if !bash_out.stderr.is_empty() {
                                            eprint!(
                                                "Bash stderr: {}",
                                                String::from_utf8_lossy(&bash_out.stderr)
                                            );
                                        }
                                        println!("Bash exit code: {}", bash_out.status);
                                    }
                                    (Err(perl_err), Err(bash_err)) => {
                                        // Both failed
                                        println!("Perl execution failed: {}", perl_err);
                                        println!("Bash execution failed: {}", bash_err);
                                    }
                                }

                                // Clean up temporary file
                                let _ = fs::remove_file(tmp_file);
                            } else {
                                println!("Error writing temporary Perl file");
                            }
                        }
                        Err(e) => {
                            eprintln!("Parse error: {}", e);
                            eprintln!("Use '{} --help' for usage information", args[0]);
                            std::process::exit(1);
                        }
                    }

                    println!("{}", "=".repeat(50));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use debashl::lexer::{Lexer, Token};

    #[test]
    fn test_lexer_basic() {
        let input = "echo hello world";
        let mut lexer = Lexer::new(input);

        assert_eq!(lexer.next(), Some(&Token::Identifier));
        assert_eq!(lexer.next(), Some(&Token::Space));
        assert_eq!(lexer.next(), Some(&Token::Identifier));
        assert_eq!(lexer.next(), Some(&Token::Space));
        assert_eq!(lexer.next(), Some(&Token::Identifier));
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_parser_basic() {
        let input = "echo hello world";
        let result = Parser::new(input).parse();
        assert!(result.is_ok());
    }
}
