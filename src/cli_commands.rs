use std::fs;
use std::process::Command;
use std::io::Write;
use debashl::shared_utils;
use crate::utils::{extract_line_col, caret_snippet};
use debashl::{Lexer, Parser, Generator};
use debashl::mir_simple::{MirCommand, MirWord};
use debashl::ast::Word;

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