use crate::ast::*;
use crate::lexer::{Lexer, Token};
use crate::parser::errors::ParserError;
use crate::parser::utilities::ParserUtilities;
use crate::parser::words::parse_word;

pub fn parse_environment_variable_value(lexer: &mut Lexer) -> Result<Word, ParserError> {
    if let Some(tok) = lexer.peek() {
        match tok {
            Token::Arithmetic | Token::ArithmeticEval => {
                // Parse arithmetic expression properly
                parse_arithmetic_expression(lexer)
            }
            Token::DollarParen => {
                // Parse variable expansion
                parse_variable_expansion(lexer)
            }
            Token::ParenOpen => {
                // Parse parenthetical text as a literal
                let text = lexer.capture_parenthetical_text()?;
                Ok(Word::literal(text))
            }
            Token::DoubleQuotedString | Token::SingleQuotedString => {
                // Parse quoted string as a literal
                let text = lexer.get_string_text()?;
                Ok(Word::literal(text))
            }
            Token::BacktickString => {
                // Parse backtick string as a literal
                let text = lexer.get_raw_token_text()?;
                Ok(Word::literal(text))
            }
            _ => {
                // Parse as a literal string until separator
                let mut value = String::new();
                loop {
                    match lexer.peek() {
                        Some(Token::Space)
                        | Some(Token::Tab)
                        | Some(Token::Newline)
                        | Some(Token::Semicolon)
                        | None => break,
                        Some(Token::Arithmetic) | Some(Token::ArithmeticEval) => {
                            // Parse arithmetic expression properly
                            return parse_arithmetic_expression(lexer);
                        }
                        Some(Token::DollarParen) => {
                            // Parse variable expansion
                            return parse_variable_expansion(lexer);
                        }
                        Some(Token::ParenOpen) => {
                            // Parse parenthetical text as a literal
                            let text = lexer.capture_parenthetical_text()?;
                            value.push_str(&text);
                        }
                        _ => {
                            if let Some((start, end)) = lexer.get_span() {
                                value.push_str(&lexer.get_text(start, end));
                                lexer.next();
                            } else {
                                break;
                            }
                        }
                    }
                }
                Ok(Word::literal(value))
            }
        }
    } else {
        Ok(Word::literal(String::new()))
    }
}

pub fn parse_array_elements(lexer: &mut Lexer) -> Result<Vec<Word>, ParserError> {
    // Array literals `arr=(...)` (core request posix-sh-go-20260806-174619):
    // each element is a REAL Word so the quoted/unquoted distinction
    // survives into the A1 contract (unquoted `$x` → split(getVar("x")) —
    // bash field-splits it; quoted `"$x"` stays a single element). The old
    // raw-text scanner destroyed the distinction before ast_to_ir.
    let mut elements = Vec::new();
    let mut loop_count = 0;

    // Skip the opening parenthesis if it's the first token
    if matches!(lexer.peek(), Some(Token::ParenOpen)) {
        lexer.next(); // consume (
    }

    loop {
        loop_count += 1;
        if loop_count > 10000 {
            return Err(ParserError::InvalidSyntax(
                "Array parsing loop limit exceeded".to_string(),
            ));
        }

        // Space/Tab/Newline separate elements — Newline is WHITESPACE inside
        // an array literal (unlike parse_word_list, where it terminates the
        // list). Comments are skipped like parse_word_list does.
        lexer.skip_whitespace_and_comments();

        match lexer.peek() {
            None => {
                // Unterminated array literal: end of tokens reached
                break;
            }
            Some(Token::ParenClose) => {
                lexer.next(); // consume )
                break;
            }
            Some(_) => {
                let word = parse_word(lexer)?;
                elements.push(word);
                // A `$(...)` element whose closing `)` directly touches the
                // array's closing `)` arrives as ONE ArithmeticEvalClose
                // token (the lexer merges `))`; see
                // capture_parenthetical_text). The cmdsub capture consumed
                // the array's `)` too, so the array is closed — stop
                // parsing elements (mirrors the raw-text scanner's early
                // return for the same shape).
                if matches!(
                    lexer
                        .current
                        .checked_sub(1)
                        .and_then(|i| lexer.tokens.get(i)),
                    Some((Token::ArithmeticEvalClose, _, _))
                ) {
                    break;
                }
            }
        }
    }
    Ok(elements)
}

pub fn parse_word_list(lexer: &mut Lexer) -> Result<Vec<Word>, ParserError> {
    let mut words = Vec::new();

    loop {
        // Skip whitespace and comments
        lexer.skip_whitespace_and_comments();

        // Check for end of list
        if lexer.is_eof()
            || matches!(
                lexer.peek(),
                Some(
                    Token::Semicolon
                        | Token::Newline
                        | Token::CarriageReturn
                        | Token::Done
                        | Token::ParenClose
                        | Token::BraceClose
                )
            )
        {
            break;
        }

        // Parse the next word
        let word = parse_word(lexer)?;
        words.push(word);

        // Skip whitespace after the word
        lexer.skip_whitespace_and_comments();
    }

    Ok(words)
}

// Placeholder functions - these would need to be implemented based on the actual AST structures
fn parse_arithmetic_expression(lexer: &mut Lexer) -> Result<Word, ParserError> {
    // Handle arithmetic expressions like $((i + 1))
    // First, consume the opening $(( or $( token
    match lexer.peek() {
        Some(Token::Arithmetic) | Some(Token::ArithmeticEval) => {
            lexer.next(); // consume $(( or $(
        }
        _ => {
            return Err(ParserError::InvalidSyntax(
                "Expected arithmetic expression start".to_string(),
            ));
        }
    }

    let mut expression_parts = Vec::new();
    let mut paren_depth = 2; // $(( or (( contributes 2 opening parens

    loop {
        match lexer.peek() {
            Some(Token::ArithmeticEvalClose) => {
                // ArithmeticEvalClose represents TWO closing parens.
                // Only push `)` that close inner (expression) parens,
                // not those that close the outer $(( or (( marker.
                // Inner parens keep depth >= 2 (the 2 from $((/(()).
                lexer.next();
                let inner_count = std::cmp::max(0, paren_depth - 2);
                paren_depth -= 2;
                for _ in 0..inner_count {
                    expression_parts.push(")".to_string());
                }
                if paren_depth <= 0 {
                    break;
                }
            }
            Some(Token::ParenOpen) => {
                if let Some(text) = lexer.get_current_text() {
                    expression_parts.push(text);
                }
                lexer.next();
                paren_depth += 1;
            }
            Some(Token::ParenClose) => {
                // Only push `)` if it closes an inner (expression) paren,
                // not if it closes the outer $(( or (( marker.
                // Inner parens keep depth >= 2 (the 2 from $((/(()).
                paren_depth -= 1;
                if paren_depth >= 2 {
                    if let Some(text) = lexer.get_current_text() {
                        expression_parts.push(text);
                    }
                }
                lexer.next();
                if paren_depth <= 0 {
                    break;
                }
            }
            Some(Token::Arithmetic) | Some(Token::ArithmeticEval) => {
                // Nested (( or $((
                if let Some(text) = lexer.get_current_text() {
                    expression_parts.push(text);
                }
                lexer.next();
                paren_depth += 2;
            }
            Some(Token::Identifier) => {
                let var_name = lexer.get_identifier_text()?;
                expression_parts.push(var_name);
                lexer.next(); // consume the identifier token
            }
            Some(Token::Number) => {
                let num_text = lexer.get_number_text()?;
                expression_parts.push(num_text);
                lexer.next(); // consume the number token
            }
            Some(Token::Plus) => {
                // Plus operator
                lexer.next();
                expression_parts.push("+".to_string());
            }
            Some(Token::Minus) => {
                // Minus operator
                lexer.next();
                expression_parts.push("-".to_string());
            }
            Some(Token::Star) => {
                // Multiplication operator
                lexer.next();
                expression_parts.push("*".to_string());
            }
            Some(Token::Slash) => {
                // Division operator
                lexer.next();
                expression_parts.push("/".to_string());
            }
            Some(Token::Space) | Some(Token::Tab) => {
                // Skip whitespace
                lexer.next();
            }
            Some(Token::DollarParen) => {
                // Nested $(...) command substitution inside arithmetic
                paren_depth += 1;
                if let Some(text) = lexer.get_current_text() {
                    expression_parts.push(text);
                }
                lexer.next();
            }
            Some(Token::Dollar) => {
                // Handle variable references like $i
                lexer.next();
                if let Some(Token::Identifier) = lexer.peek() {
                    let var_name = lexer.get_identifier_text()?;
                    expression_parts.push(format!("${}", var_name));
                } else {
                    return Err(ParserError::InvalidSyntax(
                        "Expected identifier after $ in arithmetic expression".to_string(),
                    ));
                }
            }
            None => {
                return Err(ParserError::InvalidSyntax(
                    "Unexpected end of input in arithmetic expression".to_string(),
                ));
            }
            Some(Token::Comment) => {
                // A `#` inside an arithmetic expression is the base-notation operator
                // (e.g. 10#$x), not a comment start.  Use scan_arithmetic_comment to
                // extract the content before `))` and inject `))` + remaining text.
                // The normal ArithmeticEvalClose case handles the `))`.
                let captured = lexer.scan_arithmetic_comment();
                expression_parts.push(captured);
            }
            _ => {
                // For any other token, just consume it and add its text
                if let Some(text) = lexer.get_current_text() {
                    expression_parts.push(text);
                    lexer.next();
                } else {
                    break;
                }
            }
        }
    }

    let expression = expression_parts.join("");

    // Return as an Arithmetic Word variant
    Ok(Word::arithmetic(ArithmeticExpression {
        expression,
        tokens: vec![], // We don't need to store individual tokens for now
    }))
}

fn parse_variable_expansion(_lexer: &mut Lexer) -> Result<Word, ParserError> {
    // TODO: Implement variable expansion parsing
    Err(ParserError::InvalidSyntax(
        "Variable expansion not yet implemented".to_string(),
    ))
}
