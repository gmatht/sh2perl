use debashl::lexer::{Lexer, Token};

#[test]
fn debug_tokens() {
    let input = "complex_function --flag1 --option1=value1 -abc\n";
    let lexer = Lexer::new(input);
    for (i, (token, start, end)) in lexer.tokens.iter().enumerate() {
        println!("  {}: {:?} = {:?}", i, token, &input[*start..*end]);
    }
}
