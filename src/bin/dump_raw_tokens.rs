use std::env;
use std::fs;
use logos::Logos;
use debashl::lexer::Token;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: dump_raw_tokens <file>");
        std::process::exit(1);
    }
    let input = fs::read_to_string(&args[1]).expect("Cannot read file");
    
    let mut lexer = Token::lexer(&input);
    println!("=== Raw logos tokens (before post-processing) ===");
    while let Some(token_result) = lexer.next() {
        let span = lexer.span();
        let text = &input[span.start..span.end];
        let text_escaped: String = text.chars().map(|c| match c {
            '\n' => "\\n".to_string(),
            '\r' => "\\r".to_string(),
            '\t' => "\\t".to_string(),
            ' ' => " ".to_string(),
            _ => c.to_string(),
        }).collect();
        match token_result {
            Ok(token) => println!("  {:30} start={:4} end={:4} text={:?}", format!("{:?}", token), span.start, span.end, text_escaped),
            Err(_) => println!("  ERROR at {:4}..{:4}", span.start, span.end),
        }
    }
}
