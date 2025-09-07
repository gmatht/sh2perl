mod cache;
mod execution;
mod utils;
mod testing;
mod cli_commands;
mod help;

use std::env;
use std::fs;

// Use the debug module for controlling DEBUG output
use debashl::debug::set_debug_enabled;
use debashl::{Parser, Generator, shared_utils::SharedUtils};

// Import from our new modules
use crate::utils::generate_unified_diff;
use crate::testing::{test_all_examples, test_all_examples_next_fail, find_uses_of_system,
                    test_file_equivalence, AstFormatOptions};
use crate::cli_commands::{run_generated, lex_input, parse_input, parse_file, parse_to_perl, 
                     parse_file_to_perl, parse_system_to_perl, parse_backticks_to_perl, interactive_mode, export_mir, parse_perl_critic_only};
use crate::help::show_help;

fn main() {
    let args: Vec<String> = env::args().collect();
    let program_name = &args[0];
    
    if args.len() < 2 {
        show_help(program_name);
        return;
    }
    
    let mut command = &args[1];
    
    if command == "--help" || command == "-h" {
        show_help(&args[0]);
        return;
    }
    
    // Check for debug control flags early
    if command == "--debug" {
        set_debug_enabled(true);
    } else if command == "--no-debug" {
        set_debug_enabled(false);
    } else if command == "--next-fail" {
        set_debug_enabled(false);
    }
    
    // Parse AST formatting options and input/output options
    let mut ast_options = AstFormatOptions::default();
    let mut input_file: Option<String> = None;
    let mut output_file: Option<String> = None;
    let mut optimize_mir = false;
    let mut enable_perl_critic = false;
    let mut perl_critic_only = false;
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
                perl_critic_only = true;
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
            match fs::read_to_string(input_filename) {
                Ok(content) => {
                    println!("Processing input file: {}", input_filename);
                    // Parse the shell script
                    let commands = match Parser::new(&content).parse() {
                        Ok(c) => c,
                        Err(e) => { 
                            println!("Parse error: {}", e); 
                            return; 
                        }
                    };
                    
                    // Generate Perl code
                    let mut gen = Generator::new();
                    let code = gen.generate(&commands);
                    
                    // Handle output file option
                    if let Some(output_filename) = &output_file {
                        // Write to output file with UTF-8 encoding
                        match SharedUtils::write_utf8_file(output_filename, &code) {
                            Ok(_) => println!("Generated Perl code written to: {} (UTF-8 encoded)", output_filename),
                            Err(e) => println!("Error writing to output file {}: {}", output_filename, e),
                        }
                    } else {
                        // Show generated code and run it
                        println!("Generated Perl code:");
                        println!("{}", code);
                        println!("\n--- Running generated Perl code ---");
                        let tmp = "__tmp_run.pl";
                        if SharedUtils::write_utf8_file(tmp, &code).is_ok() {
                            let _ = std::process::Command::new("perl").arg(tmp).status();
                            let _ = fs::remove_file(tmp);
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
                    "--ast-pretty" | "--ast-compact" | "--ast-indent" | "--ast-no-indent" | 
                    "--ast-newlines" | "--ast-no-newlines" => {
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
                            println!("Warning: Only 'perl' generator is supported, skipping '{}'", generator);
                        }
                    }
                }
                i += 1;
            }
            
            // If no generators specified, default to perl
            if generators.is_empty() {
                generators = vec!["perl".to_string()];
            }
            
            test_all_examples_next_fail(&generators, test_prefix, enable_perl_critic);
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
            println!("{}", generate_unified_diff(&content1, &content2, file1, file2));
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
                parse_to_perl(input);
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
                let _ = test_file_equivalence(lang, filename);
            } else {
                println!("Error: Only 'perl' language is supported");
                return;
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
                    "--ast-pretty" | "--ast-compact" | "--ast-indent" | "--ast-no-indent" | 
                    "--ast-newlines" | "--ast-no-newlines" => {
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
            
            test_all_examples_next_fail(&generators, test_prefix, enable_perl_critic);
        }
        _ => {
            // Handle input file option
            if let Some(input_filename) = &input_file {
                // Always treat as input file when -i is specified
                match fs::read_to_string(input_filename) {
                    Ok(content) => {
                        println!("Processing input file: {}", input_filename);
                        // Parse the shell script
                        let commands = match Parser::new(&content).parse() {
                            Ok(c) => c,
                            Err(e) => { 
                                println!("Parse error: {}", e); 
                                return; 
                            }
                        };
                        
                        // Generate Perl code
                        let mut gen = Generator::new();
                        let code = gen.generate(&commands);
                        
                        // Handle output file option
                        if let Some(output_filename) = &output_file {
                            // Write to output file with UTF-8 encoding
                            match SharedUtils::write_utf8_file(output_filename, &code) {
                                Ok(_) => println!("Generated Perl code written to: {} (UTF-8 encoded)", output_filename),
                                Err(e) => println!("Error writing to output file {}: {}", output_filename, e),
                            }
                        } else {
                            // Show generated code and run it
                            println!("Generated Perl code:");
                            println!("{}", code);
                            println!("\n--- Running generated Perl code ---");
                            let tmp = "__tmp_run.pl";
                            if SharedUtils::write_utf8_file(tmp, &code).is_ok() {
                                let _ = std::process::Command::new("perl").arg(tmp).status();
                                let _ = fs::remove_file(tmp);
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
                                println!("Parse error: {}", e); 
                                return; 
                            }
                        };
                        
                        // Generate Perl code
                        let mut gen = Generator::new();
                        let code = gen.generate(&commands);
                        
                        // Handle output file option
                        if let Some(output_filename) = &output_file {
                            // Write to output file with UTF-8 encoding
                            match SharedUtils::write_utf8_file(output_filename, &code) {
                                Ok(_) => println!("Generated Perl code written to: {} (UTF-8 encoded)", output_filename),
                                Err(e) => println!("Error writing to output file {}: {}", output_filename, e),
                            }
                        } else {
                            // Show generated code and run it
                            println!("Generated Perl code:");
                            println!("{}", code);
                            println!("\n--- Running generated Perl code ---");
                            let tmp = "__tmp_run.pl";
                            if SharedUtils::write_utf8_file(tmp, &code).is_ok() {
                                // Time the Perl execution
                                let perl_start = std::time::Instant::now();
                                let perl_output = std::process::Command::new("perl").arg(tmp).output();
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
                                        let perl_stdout = String::from_utf8_lossy(&perl_out.stdout).to_string();
                                        let perl_stderr = String::from_utf8_lossy(&perl_out.stderr).to_string();
                                        let bash_stdout = String::from_utf8_lossy(&bash_out.stdout).to_string();
                                        let bash_stderr = String::from_utf8_lossy(&bash_out.stderr).to_string();
                                        
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
                                        println!("Perl execution time:  {:.4} seconds", perl_duration.as_secs_f64());
                                        println!("Bash execution time:  {:.4} seconds", bash_duration.as_secs_f64());
                                        
                                        let speedup = if perl_duration.as_secs_f64() > 0.0 {
                                            bash_duration.as_secs_f64() / perl_duration.as_secs_f64()
                                        } else {
                                            0.0
                                        };
                                        
                                        if speedup > 1.0 {
                                            println!("Perl is {:.2}x faster than Bash", speedup);
                                        } else if speedup > 0.0 {
                                            println!("Bash is {:.2}x faster than Perl", 1.0 / speedup);
                                        } else {
                                            println!("Cannot calculate speedup (Perl execution time was 0)");
                                        }
                                        
                                        // Display diff output
                                        println!("\n{}", "=".repeat(50));
                                        println!("OUTPUT COMPARISON");
                                        println!("{}", "=".repeat(50));
                                        
                                        let stdout_match = perl_stdout.trim() == bash_stdout.trim();
                                        let stderr_match = perl_stderr.trim() == bash_stderr.trim();
                                        let exit_match = perl_out.status.code() == bash_out.status.code();
                                        
                                        if stdout_match && stderr_match && exit_match {
                                            println!("✓ PERFECT MATCH: Perl and Bash outputs are identical!");
                                        } else {
                                            println!("✗ DIFFERENCES FOUND:");
                                            
                                            if !stdout_match {
                                                println!("\nSTDOUT DIFFERENCES:");
                                                println!("{}", generate_unified_diff(&bash_stdout, &perl_stdout, "bash_stdout", "perl_stdout"));
                                            }
                                            
                                            if !stderr_match {
                                                println!("\nSTDERR DIFFERENCES:");
                                                println!("{}", generate_unified_diff(&bash_stderr, &perl_stderr, "bash_stderr", "perl_stderr"));
                                            }
                                            
                                            if !exit_match {
                                                println!("\nEXIT CODE DIFFERENCES:");
                                                println!("Bash exit code: {:?}", bash_out.status.code());
                                                println!("Perl exit code: {:?}", perl_out.status.code());
                                            }
                                        }
                                    }
                                    (Ok(perl_out), Err(bash_err)) => {
                                        // Perl succeeded but bash failed
                                        if !perl_out.stdout.is_empty() {
                                            print!("{}", String::from_utf8_lossy(&perl_out.stdout));
                                        }
                                        if !perl_out.stderr.is_empty() {
                                            eprint!("{}", String::from_utf8_lossy(&perl_out.stderr));
                                        }
                                        println!("Exit code: {}", perl_out.status);
                                        println!("\nBash execution failed: {}", bash_err);
                                    }
                                    (Err(perl_err), Ok(bash_out)) => {
                                        // Bash succeeded but Perl failed
                                        println!("Perl execution failed: {}", perl_err);
                                        if !bash_out.stdout.is_empty() {
                                            print!("Bash output: {}", String::from_utf8_lossy(&bash_out.stdout));
                                        }
                                        if !bash_out.stderr.is_empty() {
                                            eprint!("Bash stderr: {}", String::from_utf8_lossy(&bash_out.stderr));
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
                // Treat unknown commands as shell commands to be executed with timing and diff
                println!("Executing shell command: {}", command);
                println!("{}", "=".repeat(50));
                
                // Parse the command as shell input
                match Parser::new(command).parse() {
                    Ok(commands) => {
                        // Generate Perl code
                        let mut generator = Generator::new();
                        let perl_code = generator.generate(&commands);
                        
                        // Write to temporary file and execute
                        let tmp_file = "__tmp_direct_exec.pl";
                        if SharedUtils::write_utf8_file(tmp_file, &perl_code).is_ok() {
                            println!("Generated Perl code:");
                            println!("{}", perl_code);
                            println!("\n--- Running generated Perl code ---");
                            
                            // Time the Perl execution
                            let perl_start = std::time::Instant::now();
                            let perl_output = std::process::Command::new("perl").arg(tmp_file).output();
                            let perl_duration = perl_start.elapsed();
                            
                            // Time the bash execution
                            let bash_start = std::time::Instant::now();
                            
                            // Remove single quotes from the command if present
                            let bash_command = if command.starts_with("'") && command.ends_with("'") {
                                &command[1..command.len()-1]
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
                                    let perl_stdout = String::from_utf8_lossy(&perl_out.stdout).to_string();
                                    let perl_stderr = String::from_utf8_lossy(&perl_out.stderr).to_string();
                                    let bash_stdout = String::from_utf8_lossy(&bash_out.stdout).to_string();
                                    let bash_stderr = String::from_utf8_lossy(&bash_out.stderr).to_string();
                                    
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
                                    println!("Perl execution time:  {:.4} seconds", perl_duration.as_secs_f64());
                                    println!("Bash execution time:  {:.4} seconds", bash_duration.as_secs_f64());
                                    
                                    let speedup = if perl_duration.as_secs_f64() > 0.0 {
                                        bash_duration.as_secs_f64() / perl_duration.as_secs_f64()
                                    } else {
                                        0.0
                                    };
                                    
                                    if speedup > 1.0 {
                                        println!("Perl is {:.2}x faster than Bash", speedup);
                                    } else if speedup > 0.0 {
                                        println!("Bash is {:.2}x faster than Perl", 1.0 / speedup);
                                    } else {
                                        println!("Cannot calculate speedup (Perl execution time was 0)");
                                    }
                                    
                                    // Display diff output
                                    println!("\n{}", "=".repeat(50));
                                    println!("OUTPUT COMPARISON");
                                    println!("{}", "=".repeat(50));
                                    
                                    let stdout_match = perl_stdout.trim() == bash_stdout.trim();
                                    let stderr_match = perl_stderr.trim() == bash_stderr.trim();
                                    let exit_match = perl_out.status.code() == bash_out.status.code();
                                    
                                    if stdout_match && stderr_match && exit_match {
                                        println!("✓ PERFECT MATCH: Perl and Bash outputs are identical!");
                                    } else {
                                        println!("✗ DIFFERENCES FOUND:");
                                        
                                        if !stdout_match {
                                            println!("\nSTDOUT DIFFERENCES:");
                                            println!("{}", generate_unified_diff(&bash_stdout, &perl_stdout, "bash_stdout", "perl_stdout"));
                                        }
                                        
                                        if !stderr_match {
                                            println!("\nSTDERR DIFFERENCES:");
                                            println!("{}", generate_unified_diff(&bash_stderr, &perl_stderr, "bash_stderr", "perl_stderr"));
                                        }
                                        
                                        if !exit_match {
                                            println!("\nEXIT CODE DIFFERENCES:");
                                            println!("Bash exit code: {:?}", bash_out.status.code());
                                            println!("Perl exit code: {:?}", perl_out.status.code());
                                        }
                                    }
                                }
                                (Ok(perl_out), Err(bash_err)) => {
                                    // Perl succeeded but bash failed
                                    if !perl_out.stdout.is_empty() {
                                        print!("{}", String::from_utf8_lossy(&perl_out.stdout));
                                    }
                                    if !perl_out.stderr.is_empty() {
                                        eprint!("{}", String::from_utf8_lossy(&perl_out.stderr));
                                    }
                                    println!("Exit code: {}", perl_out.status);
                                    println!("\nBash execution failed: {}", bash_err);
                                }
                                (Err(perl_err), Ok(bash_out)) => {
                                    // Bash succeeded but Perl failed
                                    println!("Perl execution failed: {}", perl_err);
                                    if !bash_out.stdout.is_empty() {
                                        print!("Bash output: {}", String::from_utf8_lossy(&bash_out.stdout));
                                    }
                                    if !bash_out.stderr.is_empty() {
                                        eprint!("Bash stderr: {}", String::from_utf8_lossy(&bash_out.stderr));
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
                        println!("Parse error: {}", e);
                        println!("Use '{} --help' for usage information", args[0]);
                    }
                }
                
                println!("{}", "=".repeat(50));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::{Lexer, Token};

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
