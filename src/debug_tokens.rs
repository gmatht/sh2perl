use debashl::lexer::{Lexer, Token};

#[test]
fn debug_tokens() {
    let input = "case ${0##*/} in\n  *cmp*) prog=xzcmp;;\n  *) prog=xzdiff;;\nesac\n";
    println!("Input: {:?}", input);
    let lexer = Lexer::new(input);
    for (i, (token, start, end)) in lexer.tokens.iter().enumerate() {
        if *start < input.len() && *end <= input.len() {
            let text: String = input[*start..*end].chars().map(|c| if c == '\n' { '⏎' } else { c }).collect();
            println!("  {}: {:?} = {:?} (bytes {}-{})", i, token, text, start, end);
        }
    }
}
