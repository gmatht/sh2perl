use std::env;
use std::fs;
use logos::Logos;
use debashl::lexer::{Lexer, Token};
use debashl::parser::utilities::ParserUtilities;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: debug_lexer_output <file>");
        std::process::exit(1);
    }
    let input = fs::read_to_string(&args[1]).expect("Cannot read file");
    
    // Dump raw logos tokens
    println!("=== Logos raw tokens ===");
    let mut logos_lexer = Token::lexer(&input);
    let mut raw_tokens: Vec<(Token, usize, usize)> = Vec::new();
    while let Some(token_result) = logos_lexer.next() {
        let span = logos_lexer.span();
        match token_result {
            Ok(token) => {
                raw_tokens.push((token, span.start, span.end));
            }
            Err(_) => {
                println!("  ERR at bytes {}-{}", span.start, span.end);
            }
        }
    }
    for (i, (token, start, end)) in raw_tokens.iter().enumerate() {
        let text = if *end > *start && *start < input.len() && *end <= input.len() {
            &input[*start..*end]
        } else {
            ""
        };
        let escaped: String = text.chars().map(|c| match c {
            '\n' => "\\n".to_string(),
            '\r' => "\\r".to_string(),
            '\t' => "\\t".to_string(),
            ' ' => "·".to_string(),
            _ => c.to_string(),
        }).collect();
        println!("  {:3}: {:30} [{:3}, {:3}] {:?}", i, format!("{:?}", token), start, end, escaped);
    }
    println!("Total raw tokens: {}", raw_tokens.len());
    
    // Dump Lexer tokens (after post-processing)
    println!("\n=== Lexer tokens (after post-processing) ===");
    let lexer = Lexer::new(&input);
    for (i, (token, start, end)) in lexer.tokens.iter().enumerate() {
        let text = if *end > *start && *start < input.len() && *end <= input.len() {
            &input[*start..*end]
        } else {
            ""
        };
        let escaped: String = text.chars().map(|c| match c {
            '\n' => "\\n".to_string(),
            '\r' => "\\r".to_string(),
            '\t' => "\\t".to_string(),
            ' ' => "·".to_string(),
            _ => c.to_string(),
        }).collect();
        println!("  {:3}: {:30} [{:3}, {:3}] {:?}", i, format!("{:?}", token), start, end, escaped);
    }
    println!("Total lexer tokens: {}", lexer.tokens.len());
    println!("Input length: {}", input.len());
}
