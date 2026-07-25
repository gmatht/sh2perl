use debashl::lexer::{Lexer, Token};
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <file>", args[0]);
        return;
    }
    let input = fs::read_to_string(&args[1]).unwrap_or_else(|_| args[1].clone());
    println!("Input ({}, {} bytes): {:?}", input.len(), input.len(), &input[..input.len().min(200)]);
    let lexer = Lexer::new(&input);
    for (i, (token, start, end)) in lexer.tokens.iter().enumerate() {
        if *start < input.len() && *end <= input.len() {
            let text: String = input[*start..*end].chars().map(|c| if c == '\n' { '⏎' } else { if c == '\r' { '¶' } else { c } }).collect();
            println!("  {:>4}: {:30} = {:?} ({}-{})", i, format!("{:?}", token), text, start, end);
        } else {
            println!("  {:>4}: {:30} = <OOB> ({}, {})", i, format!("{:?}", token), start, end);
        }
    }
}
