use std::env;
use std::fs;
use debashl::lexer::{Lexer, Token};
use logos::Logos;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: dump_tokens_fixed <file>");
        std::process::exit(1);
    }
    let input = fs::read_to_string(&args[1]).expect("Cannot read file");
    
    let lexer = Lexer::new(&input);
    for (i, (token, start, end)) in lexer.tokens.iter().enumerate() {
        let text = &input[*start..*end];
        let text_escaped: String = text.chars().map(|c| match c {
            '\n' => "\\n".to_string(),
            '\r' => "\\r".to_string(),
            '\t' => "\\t".to_string(),
            _ => c.to_string(),
        }).collect();
        println!("{:6}: {:40} start={:6} end={:6} text={:?}", i, format!("{:?}", token), start, end, text_escaped);
    }
}
