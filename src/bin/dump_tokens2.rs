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
    let bytes = input.as_bytes();
    let lexer = Lexer::new(&input);
    for (i, (token, start, end)) in lexer.tokens.iter().enumerate() {
        let text = &input[*start..*end];
        let text_escaped: String = text.chars().map(|c| match c {
            '\n' => "\\n".to_string(),
            '\r' => "\\r".to_string(),
            '\t' => "\\t".to_string(),
            ' ' => " ".to_string(),
            _ => c.to_string(),
        }).collect();
        println!("{:4}: {:30} start={:4} end={:4} text={:?}", i, format!("{:?}", token), start, end, text_escaped);
    }
    
    // Also print byte-by-byte for the relevant region
    println!("\nByte dump of input:");
    for (i, &b) in bytes.iter().enumerate() {
        let c = if b >= 32 && b < 127 { b as char } else { '.' };
        println!("  {:4}: {:3} {:?} {}", i, b, c as char, if c == '\\' { "BACKSLASH" } else if b == b'\n' { "NEWLINE" } else if b == b'"' { "QUOTE" } else if b == b'$' { "DOLLAR" } else if b == b'(' { "LPAREN" } else if b == b')' { "RPAREN" } else { "" });
    }
}
