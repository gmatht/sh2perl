use std::env;
use std::fs;
use debashl::lexer::{Lexer, Token};
use logos::Logos;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: dump_raw <file>");
        std::process::exit(1);
    }
    let input = fs::read_to_string(&args[1]).expect("Cannot read file");
    
    // Simulate what Lexer::new does but stop before post-processing
    let mut tokens = Vec::new();
    let mut lexer = Token::lexer(&input);

    while let Some(token_result) = lexer.next() {
        let span = lexer.span();
        match token_result {
            Ok(token) => tokens.push((token, span.start, span.end)),
            Err(_) => {
                continue;
            }
        }
    }
    
    for (i, (token, start, end)) in tokens.iter().enumerate() {
        if matches!(token, Token::SingleQuote | Token::SingleQuotedString | Token::Escape | Token::Dollar | Token::DollarParen | Token::DoubleQuote | Token::DoubleQuotedString) {
            let text = &input[*start..*end];
            println!("{:4}: {:30} start={:4} end={:4} text={:?}", i, format!("{:?}", token), start, end, text);
        }
    }
}
