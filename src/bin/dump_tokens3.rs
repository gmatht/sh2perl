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
    
    // Let's trace the token processing manually
    let bytes = input.as_bytes();
    
    // Step 1: Find all DoubleQuotedString tokens that logos produced
    println!("=== Step 1: Finding all \" positions ===");
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'"' {
            println!("  \" at byte {}", i);
        }
    }
    
    // Step 2: Try the regex on various substrings
    let re = regex::Regex::new(r#""([^"\\]|\\.)*""#).unwrap();
    println!("\n=== Step 2: Regex matches ===");
    for m in re.find_iter(&input) {
        let s = m.as_str();
        let escaped: String = s.chars().map(|c| match c {
            '\n' => "\\n".to_string(),
            '\r' => "\\r".to_string(),
            '\t' => "\\t".to_string(),
            _ => c.to_string(),
        }).collect();
        println!("  {:?} at bytes {}-{}", escaped, m.start(), m.end());
    }
    
    // Step 3: Check the actual Lexer tokens
    println!("\n=== Step 3: Lexer tokens ===");
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
        println!("  {:4}: {:30} start={:4} end={:4} text={:?}", i, format!("{:?}", token), start, end, text_escaped);
    }
    
    // Step 4: Check the raw logos output (before post-processing)
    // We can't easily do this, but let's simulate
    println!("\n=== Step 4: Bytes around backslash ===");
    for i in 14usize..=16 {
        if i < bytes.len() {
            println!("  byte {}: {:3} ({})", i, bytes[i], bytes[i] as char);
        }
    }
}
