use std::fs;
use std::process::Command;
use std::io::Write;
use debashl::{Lexer, Parser, Generator};
use debashl::mir_simple::MirCommand;

pub fn run_generated(lang: &str, input: &str) {
    let source = if input.ends_with(".sh") || std::path::Path::new(input).exists() {
        fs::read_to_string(input).unwrap_or_else(|_| input.to_string())
    } else { input.to_string() };

    match lang {
        "perl" => {
            let mut generator = Generator::new();
            let commands = match Parser::new(&source).parse() {
                Ok(c) => c,
                Err(e) => { 
                    println!("Parse error: {}", e); 
                    return; 
                }
            };
            let perl_code = generator.generate(&commands);
            println!("Generated Perl code:");
            println!("{}", "=".repeat(50));
            println!("{}", perl_code);
        },
        _ => println!("Unsupported language for --run: {}", lang),
    }
}

pub fn lex_input(input: &str) {
    let mut lexer = Lexer::new(input);
    let mut token_count = 0;
    
    println!("Lexing input:");
    println!("{}", "=".repeat(50));
    
    loop {
        match lexer.next() {
            Some(token) => {
                println!("{:?}", token);
                token_count += 1;
            },
            None => break,
        }
    }
    
    println!("{}", "=".repeat(50));
    println!("Total tokens: {}", token_count);
}

pub fn parse_input(input: &str) {
    let mut parser = Parser::new(input);
    
    println!("Parsing input:");
    println!("{}", "=".repeat(50));
    
    match parser.parse() {
        Ok(commands) => {
            println!("Parse successful!");
            println!("Commands: {:?}", commands);
        },
                Err(e) => {
            println!("Parse error: {}", e);
            // TODO: Fix error handling for position information
        }
    }
    
    println!("{}", "=".repeat(50));
}

pub fn parse_file(filename: &str) {
    match fs::read_to_string(filename) {
        Ok(content) => {
            parse_input(&content);
        },
        Err(e) => {
            println!("Error reading file {}: {}", filename, e);
        }
    }
}

pub fn parse_to_perl(input: &str) {
    let mut generator = Generator::new();
    
    println!("Converting to Perl:");
    println!("{}", "=".repeat(50));
    
    let commands = match Parser::new(input).parse() {
        Ok(c) => c,
        Err(e) => { 
            println!("Parse error: {}", e); 
            return; 
        }
    };
    let perl_code = generator.generate(&commands);
    println!("{}", perl_code);
    
    println!("{}", "=".repeat(50));
}

pub fn parse_to_perl_inline(input: &str) {
    let mut generator = Generator::new_inline_mode();
    
    println!("Converting to inline Perl:");
    println!("{}", "=".repeat(50));
    
    let commands = match Parser::new(input).parse() {
        Ok(c) => c,
        Err(e) => { 
            println!("Parse error: {}", e); 
            return; 
        }
    };
    let perl_code = generator.generate(&commands);
    println!("{}", perl_code);
    
    println!("{}", "=".repeat(50));
}

pub fn parse_system_to_perl(input: &str) {
    let mut generator = Generator::new();
    
    println!("Converting system command to Perl:");
    println!("{}", "=".repeat(50));
    
    // For system commands, we need to be more lenient with parsing
    // Try to parse as-is first
    let commands = match Parser::new(input).parse() {
        Ok(c) => c,
        Err(e) => { 
            // If parsing fails, try to wrap in a simple command structure
            let wrapped_input = format!("{}", input);
            match Parser::new(&wrapped_input).parse() {
                Ok(c) => c,
                Err(e2) => {
                    println!("Parse error: {}", e);
                    println!("Tried wrapped version, error: {}", e2);
                    return;
                }
            }
        }
    };
    
    let perl_code = generator.generate(&commands);
    
    // Extract preamble and core logic separately
    let (preamble, core_code) = extract_preamble_and_core(&perl_code);
    
    // Output in a format that purify.pl can parse
    if !preamble.is_empty() {
        println!("PREAMBLE:");
        println!("{}", preamble);
        println!("CORE:");
    }
    println!("{}", core_code);
    
    println!("{}", "=".repeat(50));
}

pub fn parse_backticks_to_perl(input: &str) {
    let mut generator = Generator::new();
    
    println!("Converting backticks command to Perl:");
    println!("{}", "=".repeat(50));
    
    // For backticks, we need to generate code that captures output
    let commands = match Parser::new(input).parse() {
        Ok(c) => c,
        Err(e) => { 
            // If parsing fails, try to wrap in a simple command structure
            let wrapped_input = format!("{}", input);
            match Parser::new(&wrapped_input).parse() {
                Ok(c) => c,
                Err(e2) => {
                    println!("Parse error: {}", e);
                    println!("Tried wrapped version, error: {}", e2);
                    return;
                }
            }
        }
    };
    
    let perl_code = generator.generate(&commands);
    
    // For backticks, we need to modify the output to capture it
    let clean_code = extract_backticks_perl_logic(&perl_code);
    println!("{}", clean_code);
    
    println!("{}", "=".repeat(50));
}

fn extract_core_perl_logic(perl_code: &str) -> String {
    // Look for the main logic after variable declarations
    if let Some(captures) = regex::Regex::new(r"my \$main_exit_code = 0;\s*\n(.*?)(?:\n\s*$|$)")
        .unwrap()
        .captures(perl_code) {
        let code = captures.get(1).unwrap().as_str();
        // Clean up the code - remove trailing semicolons and extra whitespace
        let cleaned = code.trim_end();
        if cleaned.ends_with(';') {
            cleaned[..cleaned.len()-1].to_string()
        } else {
            cleaned.to_string()
        }
    } else {
        // If we can't find the pattern, try to extract just the core logic
        // Look for print statements or other core logic
        if let Some(captures) = regex::Regex::new(r"(print.*?;?)\s*$")
            .unwrap()
            .captures(perl_code) {
            let code = captures.get(1).unwrap().as_str();
            code.trim_end().to_string()
        } else {
            // Return the original code if we can't extract anything
            perl_code.to_string()
        }
    }
}

fn extract_preamble_and_core(perl_code: &str) -> (String, String) {
    // Check if this is an ls command by looking for ls-specific patterns FIRST
    // (before checking for full Perl script, so ls commands get special handling)
    if perl_code.contains("@ls_files") && perl_code.contains("opendir my $dh") && perl_code.contains("$ls_dir = ") {
        // This is an ls command - generate generic preamble (just variable declarations) and extract core logic
        let preamble = "my @ls_files;\nmy $ls_dir;";
        
        // Extract the core logic (directory assignment, opendir logic, and print statement)
        if let Some(captures) = regex::Regex::new(r"(?s)\$ls_dir = '([^']+)';\s*\n@ls_files = \(\);\s*\n(.*?)(print.*?;?)\s*$")
            .unwrap()
            .captures(perl_code) {
            let dir = captures.get(1).unwrap().as_str();
            let opendir_logic = captures.get(2).unwrap().as_str().trim();
            let print_stmt = captures.get(3).unwrap().as_str();
            let core_code = format!("$ls_dir = '{}';\n@ls_files = ();\n{}\n{}", dir, opendir_logic, print_stmt);
            let final_core = if core_code.ends_with(';') {
                core_code[..core_code.len()-1].to_string()
            } else {
                core_code.to_string()
            };
            return (preamble.to_string(), final_core);
        }
        
        // Alternative pattern: look for the directory assignment in the preamble and print in core
        if let Some(captures) = regex::Regex::new(r"my \$ls_dir = '([^']+)';\n@ls_files = \(\);\n(.*?)(print.*?;?)\s*$")
            .unwrap()
            .captures(perl_code) {
            let dir = captures.get(1).unwrap().as_str();
            let opendir_logic = captures.get(2).unwrap().as_str().trim();
            let print_stmt = captures.get(3).unwrap().as_str();
            let core_code = format!("$ls_dir = '{}';\n@ls_files = ();\n{}\n{}", dir, opendir_logic, print_stmt);
            let final_core = if core_code.ends_with(';') {
                core_code[..core_code.len()-1].to_string()
            } else {
                core_code.to_string()
            };
            return (preamble.to_string(), final_core);
        }
    }
    
    // Look for the main logic after variable declarations
    if let Some(captures) = regex::Regex::new(r"(.*?my \$main_exit_code = 0;\s*\n)(.*?)(?:\n\s*$|$)")
        .unwrap()
        .captures(perl_code) {
        let preamble = captures.get(1).unwrap().as_str().trim().to_string();
        let core_code = captures.get(2).unwrap().as_str();
        // Clean up the core code - remove trailing semicolons and extra whitespace
        let cleaned = core_code.trim_end();
        let final_core = if cleaned.ends_with(';') {
            cleaned[..cleaned.len()-1].to_string()
        } else {
            cleaned.to_string()
        };
        return (preamble, final_core);
    } else {
        // Try to extract variable declarations and core logic separately
        // Look for variable declarations (my @...; or my $...;) followed by the main logic
        if let Some(captures) = regex::Regex::new(r"(?s)(.*?)(my @[^;]+;.*?)(print.*?;?)\s*$")
            .unwrap()
            .captures(perl_code) {
            let header = captures.get(1).unwrap().as_str().trim().to_string();
            let var_decls = captures.get(2).unwrap().as_str().trim().to_string();
            let core_code = captures.get(3).unwrap().as_str().trim().to_string();
            
            let preamble = if header.is_empty() {
                var_decls
            } else {
                format!("{}\n{}", header, var_decls)
            };
            
            let final_core = if core_code.ends_with(';') {
                core_code[..core_code.len()-1].to_string()
            } else {
                core_code.to_string()
            };
            
            return (preamble, final_core);
        } else {
            // If we can't find the pattern, try to extract just the core logic
            // Look for print statements or other core logic
            if let Some(captures) = regex::Regex::new(r"(print.*?;?)\s*$")
                .unwrap()
                .captures(perl_code) {
                let code = captures.get(1).unwrap().as_str();
                return ("".to_string(), code.trim_end().to_string());
            } else {
                // Return empty preamble and original code if we can't extract anything
                return ("".to_string(), perl_code.to_string());
            }
        }
    }
    
    // Check if this is a full Perl script (has shebang and use statements)
    if perl_code.contains("#!/usr/bin/env perl") && perl_code.contains("use strict") {
        // This is a full Perl script - extract preamble and core
        if let Some(captures) = regex::Regex::new(r"(?s)(#!/usr/bin/env perl.*?my \$main_exit_code = 0;\s*\n)(.*?)(?:\n\s*$|$)")
            .unwrap()
            .captures(perl_code) {
            let preamble = captures.get(1).unwrap().as_str().trim().to_string();
            let core_code = captures.get(2).unwrap().as_str().trim().to_string();
            
            let final_core = if core_code.ends_with(';') {
                core_code[..core_code.len()-1].to_string()
            } else {
                core_code.to_string()
            };
            
            return (preamble, final_core);
        }
    }
    
    // Default fallback - return original code as core with empty preamble
    ("".to_string(), perl_code.to_string())
}

fn extract_backticks_perl_logic(perl_code: &str) -> String {
    // For backticks, we need to capture the output instead of just printing it
    // Look for the main logic after variable declarations
    if let Some(captures) = regex::Regex::new(r"my \$main_exit_code = 0;\s*\n(.*?)(?:\n\s*$|$)")
        .unwrap()
        .captures(perl_code) {
        let code = captures.get(1).unwrap().as_str();
        // Convert print statements to capture output using backticks
        let modified_code = code.replace("print ", "`");
        let cleaned = modified_code.trim_end();
        if cleaned.ends_with(';') {
            let result = cleaned[..cleaned.len()-1].to_string();
            if result.ends_with('`') {
                result
            } else {
                // Remove any trailing semicolon from the command part
                let without_semicolon = result.replace(";`", "`");
                without_semicolon
            }
        } else {
            if cleaned.ends_with('`') {
                cleaned.to_string()
            } else {
                format!("{}`", cleaned)
            }
        }
    } else {
        // If we can't find the pattern, try to extract and modify print statements
        if let Some(captures) = regex::Regex::new(r"(print.*?;?)\s*$")
            .unwrap()
            .captures(perl_code) {
            let code = captures.get(1).unwrap().as_str();
            let modified_code = code.replace("print ", "`");
            let result = modified_code.trim_end().to_string();
            if result.ends_with('`') {
                result
            } else {
                let with_backtick = format!("{}`", result);
                // Remove any trailing semicolon from the command part
                with_backtick.replace(";`", "`")
            }
        } else {
            // Return the original code if we can't extract anything
            perl_code.to_string()
        }
    }
}

pub fn parse_file_to_perl(filename: &str) {
    match fs::read_to_string(filename) {
        Ok(content) => {
            parse_to_perl(&content);
        },
        Err(e) => {
            println!("Error reading file {}: {}", filename, e);
        }
    }
}

pub fn interactive_mode() {
    println!("Interactive mode - type 'quit' to exit");
    println!("{}", "=".repeat(50));
    
    loop {
        print!("> ");
        std::io::stdout().flush().unwrap();
        
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        let input = input.trim();
        
        if input == "quit" {
            break;
        }
        
        if input.is_empty() {
            continue;
        }
        
        match input {
            "help" => {
                println!("Available commands:");
                println!("  help - show this help");
                println!("  quit - exit interactive mode");
                println!("  <shell code> - parse and convert to Perl");
            },
            _ => {
                parse_to_perl(input);
            }
        }
    }
}

pub fn export_mir(input: &str, optimize: bool) {
    println!("MIR Export:");
    println!("{}", "=".repeat(50));
    
    let commands = match Parser::new(input).parse() {
        Ok(c) => c,
        Err(e) => { 
            println!("Parse error: {}", e); 
            return; 
        }
    };
    
    // Convert AST commands to MIR commands
    let mir_commands: Vec<MirCommand> = commands.iter()
        .map(|cmd| MirCommand::from_ast_command(cmd))
        .collect();
    
    if optimize {
        println!("Optimized MIR:");
        // TODO: Add optimization passes here
        for (i, mir_cmd) in mir_commands.iter().enumerate() {
            println!("Command {}: {:?}", i, mir_cmd);
        }
    } else {
        println!("MIR Commands:");
        for (i, mir_cmd) in mir_commands.iter().enumerate() {
            println!("Command {}: {:?}", i, mir_cmd);
        }
    }
    
    println!("{}", "=".repeat(50));
}

pub fn export_mir_to_json(input: &str, optimize: bool) {
    let commands = match Parser::new(input).parse() {
        Ok(c) => c,
        Err(e) => { 
            println!("Parse error: {}", e); 
            return; 
        }
    };
    
    // Convert AST commands to MIR commands
    let mir_commands: Vec<MirCommand> = commands.iter()
        .map(|cmd| MirCommand::from_ast_command(cmd))
        .collect();
    
    match serde_json::to_string_pretty(&mir_commands) {
        Ok(json) => println!("{}", json),
        Err(e) => println!("JSON serialization error: {}", e),
    }
}

pub fn parse_perl_critic_only(input: &str) {
    // Test if the input can be lexed (syntax check)
    let lex_result = test_perl_lex(input);
    if lex_result != 0 {
        std::process::exit(101);  // Lex failure
    }
    
    // Test if the input can be parsed (compilation check)
    let parse_result = test_perl_parse(input);
    if parse_result != 0 {
        std::process::exit(102);  // Parse failure
    }
    
    // Test if the input can be generated/executed
    let generate_result = test_perl_generate(input);
    if generate_result != 0 {
        std::process::exit(104);  // Generate failure
    }
    
    // Test if the generated code passes Perl Critic
    let critic_result = test_perl_critic(input);
    if critic_result != 0 {
        std::process::exit(137);  // Perl Critic failure
    }
    
    // All tests passed
    std::process::exit(0);
}

fn test_perl_lex(input: &str) -> i32 {
    // Test basic syntax with perl -c
    let mut child = Command::new("perl")
        .arg("-c")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .spawn();
    
    match child {
        Ok(mut child) => {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(input.as_bytes());
            }
            match child.wait() {
                Ok(status) => status.code().unwrap_or(1),
                Err(_) => 1
            }
        }
        Err(_) => 1
    }
}

fn test_perl_parse(input: &str) -> i32 {
    // Test compilation with perl -c (same as syntax for now)
    test_perl_lex(input)
}

fn test_perl_generate(input: &str) -> i32 {
    // Test if the code can be executed without errors
    let mut child = Command::new("perl")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .spawn();
    
    match child {
        Ok(mut child) => {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(input.as_bytes());
            }
            match child.wait() {
                Ok(status) => status.code().unwrap_or(1),
                Err(_) => 1
            }
        }
        Err(_) => 1
    }
}

fn test_perl_critic(input: &str) -> i32 {
    // Write input to temporary file
    let temp_file = "__tmp_perl_critic_test.pl";
    if let Err(_) = fs::write(temp_file, input) {
        return 1;
    }
    
    // Run Perl Critic on the file
    let output = Command::new("perl")
        .arg("perlcritic_wrapper.pl")
        .arg(temp_file)
        .output();
    
    // Clean up temporary file
    let _ = fs::remove_file(temp_file);
    
    match output {
        Ok(child) => child.status.code().unwrap_or(1),
        Err(_) => 1
    }
}