use std::env;
use std::fs;
use debashl::lexer::{Lexer, Token};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: dump_tokens <file>");
        std::process::exit(1);
    }
    let input = fs::read_to_string(&args[1]).expect("Cannot read file");
    let lexer = Lexer::new(&input);
    for (i, (token, start, end)) in lexer.tokens.iter().enumerate() {
        println!("{:4}: {:40?} = {:?}", i, token, &input[*start..*end]);
    }
}
