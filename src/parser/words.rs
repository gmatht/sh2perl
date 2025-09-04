use crate::ast::*;
use crate::lexer::{Lexer, Token};
use crate::parser::errors::ParserError;
use crate::parser::utilities::ParserUtilities;
use std::collections::HashMap;

pub fn parse_word(lexer: &mut Lexer) -> Result<Word, ParserError> {
    // Combine contiguous bare-word tokens (identifiers, numbers, slashes, dots, plus, minus, colons) into a single literal
    // This handles filenames like "file.txt" by combining Identifier + Dot + Identifier
    // and also handles find arguments like "+1M" by combining Plus + Number + Identifier
    if matches!(lexer.peek(), Some(Token::Identifier) | Some(Token::Number) | Some(Token::PaddedNumber) | Some(Token::Slash) | Some(Token::Dot) | Some(Token::Range) | Some(Token::Plus) | Some(Token::Minus) | Some(Token::Escape) | Some(Token::Colon)) {
        let mut combined = String::new();
        loop {
            match lexer.peek() {
                Some(Token::Identifier) | Some(Token::Number) | Some(Token::PaddedNumber) | Some(Token::Slash) | Some(Token::Dot) | Some(Token::Range) | Some(Token::Plus) | Some(Token::Minus) | Some(Token::Escape) | Some(Token::Colon) => {
                    // Append raw token text and consume
                    if let Some(text) = lexer.get_current_text() {
                        combined.push_str(&text);
                        lexer.next();
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
        // Skip inline whitespace after consuming the word
        lexer.skip_inline_whitespace_and_comments();
        return Ok(Word::Literal(combined, None));
    }

    let result = match lexer.peek() {
        Some(Token::Identifier) => Ok(Word::Literal(lexer.get_identifier_text()?, None)),
        Some(Token::Number) => Ok(Word::Literal(lexer.get_number_text()?, None)),
        Some(Token::PaddedNumber) => Ok(Word::Literal(lexer.get_raw_token_text()?, None)),
        Some(Token::DoubleQuotedString) => {
            // Always parse as string interpolation for double-quoted strings
            // This handles both strings and strings with variables
            Ok(parse_string_interpolation(lexer)?)
        },
        Some(Token::SingleQuotedString) => {
            let quoted_text = lexer.get_string_text()?;
            // Strip the outer quotes from single-quoted strings
            let content = if quoted_text.starts_with("'") && quoted_text.ends_with("'") {
                quoted_text[1..quoted_text.len()-1].to_string()
            } else {
                quoted_text
            };
            Ok(Word::Literal(content, None))
        },
        Some(Token::BacktickString) => parse_backtick_command_substitution(lexer),
        Some(Token::DollarSingleQuotedString) => Ok(parse_ansic_quoted_string(lexer)?),
        Some(Token::DollarDoubleQuotedString) => Ok(parse_string_interpolation(lexer)?),
        Some(Token::BraceOpen) => Ok(parse_brace_expansion(lexer)?),
        Some(Token::Source) => {
            // Treat standalone 'source' as a normal word (e.g., `source file.sh`)
            lexer.next();
            Ok(Word::Literal("source".to_string(), None))
        }
        Some(Token::Set) => {
            // Treat standalone 'set' as a normal word (e.g., `set -euo pipefail`)
            lexer.next();
            Ok(Word::Literal("set".to_string(), None))
        }
        Some(Token::Declare) => {
            // Treat standalone 'declare' as a normal word (e.g., `declare -a arr`)
            lexer.next();
            Ok(Word::Literal("declare".to_string(), None))
        }
        Some(Token::Unset) => {
            // Treat standalone 'unset' as a normal word (e.g., `unset var`)
            lexer.next();
            Ok(Word::Literal("unset".to_string(), None))
        }
        Some(Token::Export) => {
            // Treat standalone 'export' as a normal word (e.g., `export PATH`)
            lexer.next();
            Ok(Word::Literal("export".to_string(), None))
        }
        Some(Token::Readonly) => {
            // Treat standalone 'readonly' as a normal word (e.g., `readonly VAR`)
            lexer.next();
            Ok(Word::Literal("readonly".to_string(), None))
        }
        Some(Token::Typeset) => {
            // Treat standalone 'typeset' as a normal word (e.g., `typeset -i var`)
            lexer.next();
            Ok(Word::Literal("typeset".to_string(), None))
        }
        Some(Token::Local) => {
            // Treat standalone 'local' as a normal word (e.g., `local var`)
            lexer.next();
            Ok(Word::Literal("local".to_string(), None))
        }
        Some(Token::Shift) => {
            // Treat standalone 'shift' as a normal word (e.g., `shift 2`)
            lexer.next();
            Ok(Word::Literal("shift".to_string(), None))
        }
        Some(Token::Eval) => {
            // Treat standalone 'eval' as a normal word (e.g., `eval $cmd`)
            lexer.next();
            Ok(Word::Literal("eval".to_string(), None))
        }
        Some(Token::Exec) => {
            // Treat standalone 'exec' as a normal word (e.g., `exec cmd`)
            lexer.next();
            Ok(Word::Literal("exec".to_string(), None))
        }
        Some(Token::Trap) => {
            // Treat standalone 'trap' as a normal word (e.g., `trap 'echo' INT`)
            lexer.next();
            Ok(Word::Literal("trap".to_string(), None))
        }
        Some(Token::Wait) => {
            // Treat standalone 'wait' as a normal word (e.g., `wait $pid`)
            lexer.next();
            Ok(Word::Literal("wait".to_string(), None))
        }
        Some(Token::Exit) => {
            // Treat standalone 'exit' as a normal word (e.g., `exit 0`)
            lexer.next();
            Ok(Word::Literal("exit".to_string(), None))
        }
        Some(Token::Range) => {
            // Treat standalone '..' as a literal (e.g., `cd ..`)
            lexer.next();
            Ok(Word::Literal("..".to_string(), None))
        }
        Some(Token::Star) => {
            // Treat standalone '*' as a literal (e.g., `ls *`)
            lexer.next();
            Ok(Word::Literal("*".to_string(), None))
        }
        Some(Token::Dot) => {
            // Treat standalone '.' as a literal (e.g., `ls .`)
            lexer.next();
            Ok(Word::Literal(".".to_string(), None))
        }
        Some(Token::CasePattern) => {
            // Treat case statement patterns like *.txt as literals.
            // get_raw_token_text() consumes the current token, so do not call next() here.
            Ok(Word::Literal(lexer.get_raw_token_text()?, None))
        }
        Some(Token::Slash) => {
            // Treat standalone '/' as a literal (e.g., `cd /`)
            lexer.next();
            Ok(Word::Literal("/".to_string(), None))
        }
        // Test operators
        Some(Token::File) => {
            lexer.next();
            Ok(Word::Literal("-f".to_string(), None))
        }
        Some(Token::Directory) => {
            lexer.next();
            Ok(Word::Literal("-d".to_string(), None))
        }
        Some(Token::Exists) => {
            lexer.next();
            Ok(Word::Literal("-e".to_string(), None))
        }
        Some(Token::Readable) => {
            lexer.next();
            Ok(Word::Literal("-r".to_string(), None))
        }
        Some(Token::Writable) => {
            lexer.next();
            Ok(Word::Literal("-w".to_string(), None))
        }
        Some(Token::Executable) => {
            lexer.next();
            Ok(Word::Literal("-x".to_string(), None))
        }
        Some(Token::Size) => {
            lexer.next();
            Ok(Word::Literal("-s".to_string(), None))
        }
        Some(Token::Symlink) => {
            lexer.next();
            Ok(Word::Literal("-L".to_string(), None))
        }
        Some(Token::TestBracketClose) => {
            lexer.next();
            Ok(Word::Literal("]".to_string(), None))
        }
        Some(Token::Tilde) => {
            // Treat standalone '~' as a literal (e.g., `cd ~`)
            lexer.next();
            Ok(Word::Literal("~".to_string(), None))
        }
        Some(Token::LongOption) => {
            // Treat long options like --color=always as literals
            Ok(Word::Literal(lexer.get_raw_token_text()?, None))
        }
        Some(Token::RegexPattern) => {
            // Treat regex patterns as literals
            Ok(Word::Literal(lexer.get_raw_token_text()?, None))
        }
        Some(Token::RegexMatch) => {
            // Treat regex match operator as literal
            Ok(Word::Literal(lexer.get_raw_token_text()?, None))
        }
        Some(Token::NameFlag) | Some(Token::MaxDepthFlag) | Some(Token::TypeFlag) => {
            // Treat command-line flags as literals
            Ok(Word::Literal(lexer.get_raw_token_text()?, None))
        }
        Some(Token::Minus) => {
            // Handle minus tokens like -l, -c, etc.
            // Consume the minus and combine with following identifier or number if present
            lexer.next(); // consume the minus
            let mut combined = "-".to_string();
            
            // Look ahead to see if there's an identifier or number following
            if let Some(Token::Identifier) = lexer.peek() {
                let identifier = lexer.get_identifier_text()?;
                combined.push_str(&identifier);
            } else if let Some(Token::Number) = lexer.peek() {
                let number = lexer.get_number_text()?;
                combined.push_str(&number);
            }
            
            Ok(Word::Literal(combined, None))
        }
        Some(Token::Character) | Some(Token::NonZero) | Some(Token::SymlinkH) | Some(Token::PipeFile) | Some(Token::Socket) | Some(Token::Block) | Some(Token::SetGid) | Some(Token::Sticky) | Some(Token::SetUid) | Some(Token::Owned) | Some(Token::GroupOwned) | Some(Token::Modified) | Some(Token::Eq) | Some(Token::Ne) | Some(Token::Lt) | Some(Token::Le) | Some(Token::Gt) | Some(Token::Ge) | Some(Token::Zero) => {
            // Handle test operator tokens like -e, -f, -d, etc.
            // These are already complete flags, just get their text
            Ok(Word::Literal(lexer.get_raw_token_text()?, None))
        }
        Some(Token::Dollar) => Ok(parse_variable_expansion(lexer)?),
        Some(Token::DollarBrace) | Some(Token::DollarParen) | Some(Token::DollarHashSimple) | Some(Token::DollarAtSimple) | Some(Token::DollarStarSimple)
        | Some(Token::DollarBraceHash) | Some(Token::DollarBraceBang) | Some(Token::DollarBraceStar) | Some(Token::DollarBraceAt)
        | Some(Token::DollarBraceHashStar) | Some(Token::DollarBraceHashAt) | Some(Token::DollarBraceBangStar) | Some(Token::DollarBraceBangAt)
            => Ok(parse_variable_expansion(lexer)?),
        Some(Token::Arithmetic) | Some(Token::ArithmeticEval) => Ok(parse_arithmetic_expression(lexer)?),
        Some(Token::True) => {
            // Treat standalone 'true' as a normal word (e.g., `true` or `command || true`)
            lexer.next();
            Ok(Word::Literal("true".to_string(), None))
        }
        Some(Token::False) => {
            // Treat standalone 'false' as a normal word (e.g., `false` or `command && false`)
            lexer.next();
            Ok(Word::Literal("false".to_string(), None))
        }
        _ => {
            let (line, col) = lexer.offset_to_line_col(0);
            let token = lexer.peek().unwrap_or(Token::Identifier).to_owned();
            Err(ParserError::UnexpectedToken { token, line, col })
        }
    };
    
    // Skip inline whitespace after consuming the word
    lexer.skip_inline_whitespace_and_comments();
    
    result
}

/// Parse a word without skipping newlines at the end.
/// This is used specifically for argument parsing where we want to preserve newlines.
pub fn parse_word_no_newline_skip(lexer: &mut Lexer) -> Result<Word, ParserError> {
    // Combine contiguous bare-word tokens (identifiers, numbers, slashes, dots, plus, minus, colons) into a single literal
    // This handles filenames like "file.txt" by combining Identifier + Dot + Identifier
    // and also handles find arguments like "+1M" by combining Plus + Number + Identifier
    if matches!(lexer.peek(), Some(Token::Identifier) | Some(Token::Number) | Some(Token::PaddedNumber) | Some(Token::Slash) | Some(Token::Dot) | Some(Token::Range) | Some(Token::Plus) | Some(Token::Minus) | Some(Token::Escape) | Some(Token::Colon)) {
        let mut combined = String::new();
        loop {
            match lexer.peek() {
                Some(Token::Identifier) | Some(Token::Number) | Some(Token::PaddedNumber) | Some(Token::Slash) | Some(Token::Dot) | Some(Token::Range) | Some(Token::Plus) | Some(Token::Minus) | Some(Token::Escape) | Some(Token::Colon) => {
                    // Append raw token text and consume
                    if let Some(text) = lexer.get_current_text() {
                        combined.push_str(&text);
                        lexer.next();
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
        // Skip inline whitespace after consuming the word, but NOT newlines
        lexer.skip_inline_whitespace_and_comments();
        return Ok(Word::Literal(combined, None));
    }

    let result = match lexer.peek() {
        Some(Token::Identifier) => Ok(Word::Literal(lexer.get_identifier_text()?, None)),
        Some(Token::Number) => Ok(Word::Literal(lexer.get_number_text()?, None)),
        Some(Token::PaddedNumber) => Ok(Word::Literal(lexer.get_raw_token_text()?, None)),
        Some(Token::DoubleQuotedString) => {
            // Always parse as string interpolation for double-quoted strings
            // This handles both simple strings and strings with variables
            Ok(parse_string_interpolation(lexer)?)
        },
        Some(Token::SingleQuotedString) => {
            let quoted_text = lexer.get_string_text()?;
            // Strip the outer quotes from single-quoted strings
            let content = if quoted_text.starts_with("'") && quoted_text.ends_with("'") {
                quoted_text[1..quoted_text.len()-1].to_string()
            } else {
                quoted_text
            };
            Ok(Word::Literal(content, None))
        },
        Some(Token::BacktickString) => parse_backtick_command_substitution(lexer),
        Some(Token::DollarSingleQuotedString) => Ok(parse_ansic_quoted_string(lexer)?),
        Some(Token::DollarDoubleQuotedString) => Ok(parse_string_interpolation(lexer)?),
        Some(Token::BraceOpen) => Ok(parse_brace_expansion(lexer)?),
        Some(Token::Source) => {
            // Treat standalone 'source' as a normal word (e.g., `source file.sh`)
            lexer.next();
            Ok(Word::Literal("source".to_string(), None))
        }
        Some(Token::Set) => {
            // Treat standalone 'set' as a normal word (e.g., `set -euo pipefail`)
            lexer.next();
            Ok(Word::Literal("set".to_string(), None))
        }
        Some(Token::Declare) => {
            // Treat standalone 'declare' as a normal word (e.g., `declare -a arr`)
            lexer.next();
            Ok(Word::Literal("declare".to_string(), None))
        }
        Some(Token::Unset) => {
            // Treat standalone 'unset' as a normal word (e.g., `unset var`)
            lexer.next();
            Ok(Word::Literal("unset".to_string(), None))
        }
        Some(Token::Export) => {
            // Treat standalone 'export' as a normal word (e.g., `export PATH`)
            lexer.next();
            Ok(Word::Literal("export".to_string(), None))
        }
        Some(Token::Readonly) => {
            // Treat standalone 'readonly' as a normal word (e.g., `readonly VAR`)
            lexer.next();
            Ok(Word::Literal("readonly".to_string(), None))
        }
        Some(Token::Typeset) => {
            // Treat standalone 'typeset' as a normal word (e.g., `typeset -i var`)
            lexer.next();
            Ok(Word::Literal("typeset".to_string(), None))
        }
        Some(Token::Local) => {
            // Treat standalone 'local' as a normal word (e.g., `local var`)
            lexer.next();
            Ok(Word::Literal("local".to_string(), None))
        }
        Some(Token::Shift) => {
            // Treat standalone 'shift' as a normal word (e.g., `shift 2`)
            lexer.next();
            Ok(Word::Literal("shift".to_string(), None))
        }
        Some(Token::Eval) => {
            // Treat standalone 'eval' as a normal word (e.g., `eval $cmd`)
            lexer.next();
            Ok(Word::Literal("eval".to_string(), None))
        }
        Some(Token::Exec) => {
            // Treat standalone 'exec' as a normal word (e.g., `exec cmd`)
            lexer.next();
            Ok(Word::Literal("exec".to_string(), None))
        }
        Some(Token::Trap) => {
            // Treat standalone 'trap' as a normal word (e.g., `trap 'echo' INT`)
            lexer.next();
            Ok(Word::Literal("trap".to_string(), None))
        }
        Some(Token::Wait) => {
            // Treat standalone 'wait' as a normal word (e.g., `wait $pid`)
            lexer.next();
            Ok(Word::Literal("wait".to_string(), None))
        }
        Some(Token::Exit) => {
            // Treat standalone 'exit' as a normal word (e.g., `exit 0`)
            lexer.next();
            Ok(Word::Literal("exit".to_string(), None))
        }
        Some(Token::Range) => {
            // Treat standalone '..' as a literal (e.g., `cd ..`)
            lexer.next();
            Ok(Word::Literal("..".to_string(), None))
        }
        Some(Token::Star) => {
            // Treat standalone '*' as a literal (e.g., `ls *`)
            lexer.next();
            Ok(Word::Literal("*".to_string(), None))
        }
        Some(Token::Dot) => {
            // Treat standalone '.' as a literal (e.g., `ls .`)
            lexer.next();
            Ok(Word::Literal(".".to_string(), None))
        }
        Some(Token::CasePattern) => {
            // Treat case statement patterns like *.txt as literals.
            // get_raw_token_text() consumes the current token, so do not call next() here.
            Ok(Word::Literal(lexer.get_raw_token_text()?, None))
        }
        Some(Token::Slash) => {
            // Treat standalone '/' as a literal (e.g., `cd /`)
            lexer.next();
            Ok(Word::Literal("/".to_string(), None))
        }
        // Test operators
        Some(Token::File) => {
            lexer.next();
            Ok(Word::Literal("-f".to_string(), None))
        }
        Some(Token::Directory) => {
            lexer.next();
            Ok(Word::Literal("-d".to_string(), None))
        }
        Some(Token::Exists) => {
            lexer.next();
            Ok(Word::Literal("-e".to_string(), None))
        }
        Some(Token::Readable) => {
            lexer.next();
            Ok(Word::Literal("-r".to_string(), None))
        }
        Some(Token::Writable) => {
            lexer.next();
            Ok(Word::Literal("-w".to_string(), None))
        }
        Some(Token::Executable) => {
            lexer.next();
            Ok(Word::Literal("-x".to_string(), None))
        }
        Some(Token::Size) => {
            lexer.next();
            Ok(Word::Literal("-s".to_string(), None))
        }
        Some(Token::Symlink) => {
            lexer.next();
            Ok(Word::Literal("-L".to_string(), None))
        }
        Some(Token::TestBracketClose) => {
            lexer.next();
            Ok(Word::Literal("]".to_string(), None))
        }
        Some(Token::Tilde) => {
            // Treat standalone '~' as a literal (e.g., `cd ~`)
            lexer.next();
            Ok(Word::Literal("~".to_string(), None))
        }
        Some(Token::LongOption) => {
            // Treat long options like --color=always as literals
            Ok(Word::Literal(lexer.get_raw_token_text()?, None))
        }
        Some(Token::RegexPattern) => {
            // Treat regex patterns as literals
            Ok(Word::Literal(lexer.get_raw_token_text()?, None))
        }
        Some(Token::RegexMatch) => {
            // Treat regex match operator as literal
            Ok(Word::Literal(lexer.get_raw_token_text()?, None))
        }
        Some(Token::NameFlag) | Some(Token::MaxDepthFlag) | Some(Token::TypeFlag) => {
            // Treat command-line flags as literals
            Ok(Word::Literal(lexer.get_raw_token_text()?, None))
        }
        Some(Token::Minus) => {
            // Handle minus tokens like -l, -c, etc.
            // Consume the minus and combine with following identifier or number if present
            lexer.next(); // consume the minus
            let mut combined = "-".to_string();
            
            // Look ahead to see if there's an identifier or number following
            if let Some(Token::Identifier) = lexer.peek() {
                let identifier = lexer.get_identifier_text()?;
                combined.push_str(&identifier);
            } else if let Some(Token::Number) = lexer.peek() {
                let number = lexer.get_number_text()?;
                combined.push_str(&number);
            }
            
            Ok(Word::Literal(combined, None))
        }
        Some(Token::Character) | Some(Token::NonZero) | Some(Token::SymlinkH) | Some(Token::PipeFile) | Some(Token::Socket) | Some(Token::Block) | Some(Token::SetGid) | Some(Token::Sticky) | Some(Token::SetUid) | Some(Token::Owned) | Some(Token::GroupOwned) | Some(Token::Modified) | Some(Token::Eq) | Some(Token::Ne) | Some(Token::Lt) | Some(Token::Le) | Some(Token::Gt) | Some(Token::Ge) | Some(Token::Zero) => {
            // Handle test operator tokens like -e, -f, -d, etc.
            // These are already complete flags, just get their text
            Ok(Word::Literal(lexer.get_raw_token_text()?, None))
        }
        Some(Token::Dollar) => Ok(parse_variable_expansion(lexer)?),
        Some(Token::DollarBrace) | Some(Token::DollarParen) | Some(Token::DollarHashSimple) | Some(Token::DollarAtSimple) | Some(Token::DollarStarSimple)
        | Some(Token::DollarBraceHash) | Some(Token::DollarBraceBang) | Some(Token::DollarBraceStar) | Some(Token::DollarBraceAt)
        | Some(Token::DollarBraceHashStar) | Some(Token::DollarBraceHashAt) | Some(Token::DollarBraceBangStar) | Some(Token::DollarBraceBangAt)
            => Ok(parse_variable_expansion(lexer)?),
        Some(Token::Arithmetic) | Some(Token::ArithmeticEval) => Ok(parse_arithmetic_expression(lexer)?),
        Some(Token::True) => {
            // Treat standalone 'true' as a normal word (e.g., `true` or `command || true`)
            lexer.next();
            Ok(Word::Literal("true".to_string(), None))
        }
        Some(Token::False) => {
            // Treat standalone 'false' as a normal word (e.g., `false` or `command && false`)
            lexer.next();
            Ok(Word::Literal("false".to_string(), None))
        }
        _ => {
            let (line, col) = lexer.offset_to_line_col(0);
            let token = lexer.peek().unwrap_or(Token::Identifier).to_owned();
            Err(ParserError::UnexpectedToken { token, line, col })
        }
    };
    
    // Don't skip inline whitespace after consuming the word - this preserves newlines
    // for argument parsing context
    
    result
}

pub fn parse_variable_expansion(lexer: &mut Lexer) -> Result<Word, ParserError> {
    match lexer.peek() {
        Some(Token::Dollar) => {
            lexer.next();
            if let Some(Token::Identifier) = lexer.peek() {
                let var_name = lexer.get_identifier_text()?;
                
                // Check if this is followed by a bracket for array/map access like $map[key]
                if let Some(Token::TestBracket) = lexer.peek() {
                    // This is $map[key] syntax - parse the array/map access
                    lexer.next(); // consume the [
                    
                    // Parse the array index content until we find the closing ]
                    let mut index_content = String::new();
                    let mut bracket_depth = 1;
                    
                    while bracket_depth > 0 {
                        if let Some((start, end)) = lexer.get_span() {
                            let token = lexer.peek();
                            
                            match token {
                                Some(Token::TestBracket) => {
                                    bracket_depth += 1;
                                    let text = lexer.get_text(start, end);
                                    index_content.push_str(&text);
                                    lexer.next();
                                }
                                Some(Token::TestBracketClose) => {
                                    bracket_depth -= 1;
                                    if bracket_depth == 0 {
                                        // Consume the closing ]
                                        lexer.next();
                                        break;
                                    } else {
                                        let text = lexer.get_text(start, end);
                                        index_content.push_str(&text);
                                        lexer.next();
                                    }
                                }
                                Some(Token::Dollar) => {
                                    // Handle variable references in the key like $k
                                    let text = lexer.get_text(start, end);
                                    index_content.push_str(&text);
                                    lexer.next();
                                    
                                    // If followed by an identifier, consume it too
                                    if let Some(Token::Identifier) = lexer.peek() {
                                        let var_text = lexer.get_identifier_text()?;
                                        index_content.push_str(&var_text);
                                    }
                                }
                                _ => {
                                    let text = lexer.get_text(start, end);
                                    index_content.push_str(&text);
                                    lexer.next();
                                }
                            }
                        } else {
                            break;
                        }
                    }
                    
                    // Return the map access
                    return Ok(Word::MapAccess(var_name, index_content, None));
                }
                
                Ok(Word::Variable(var_name, false, None))
            } else {
                Err(ParserError::InvalidSyntax("Expected identifier after $".to_string()))
            }
        }
        Some(Token::DollarHashSimple) => { 
            lexer.next(); 
            Ok(Word::Variable("#".to_string(), false, None))
        }
        Some(Token::DollarAtSimple) => { 
            lexer.next(); 
            Ok(Word::Variable("@".to_string(), false, None))
        }
        Some(Token::DollarStarSimple) => { 
            lexer.next(); 
            Ok(Word::Variable("*".to_string(), false, None))
        }
        Some(Token::DollarBrace) => {
            // Parse ${...} expansions
            lexer.next(); // consume the token
            
            // Parse the entire braced content first, then analyze it
            let braced_content = parse_braced_variable_name(lexer)?;
            
            // Check if this is array syntax first
            if braced_content.starts_with('#') && braced_content.contains('[') && braced_content.contains(']') {
                // This is ${#arr[@]} - array length
                if let Some(bracket_start) = braced_content.find('[') {
                    if let Some(_bracket_end) = braced_content.rfind(']') {
                        let array_name = &braced_content[1..bracket_start]; // Remove # prefix
                        return Ok(Word::MapLength(array_name.to_string(), None));
                    }
                }
            } else if braced_content.starts_with('!') && braced_content.contains('[') && braced_content.contains(']') {
                // This is ${!map[@]} - get keys of associative array
                if let Some(bracket_start) = braced_content.find('[') {
                    if let Some(_bracket_end) = braced_content.rfind(']') {
                        let map_name = &braced_content[1..bracket_start]; // Remove ! prefix
                        return Ok(Word::MapKeys(map_name.to_string(), None));
                    }
                }
            } else if braced_content.contains('[') && braced_content.contains(']') {
                // This is a map/array access like ${map[foo]} or ${arr[1]} or ${map[$k]}
                if let Some(bracket_start) = braced_content.find('[') {
                    if let Some(bracket_end) = braced_content.rfind(']') {
                        let map_name = &braced_content[..bracket_start];
                        let key = &braced_content[bracket_start + 1..bracket_end];
                        
                        // Special case: if key is "@", this is array iteration
                        if key == "@" {
                            // Check if there's array slicing after the closing brace
                            // Look ahead for :offset:length syntax
                            if let Some(Token::Colon) = lexer.peek() {
                                // This is array slicing like ${arr[@]:start:length}
                                return parse_array_slicing(lexer, map_name.to_string());
                            }
                            return Ok(Word::MapAccess(map_name.to_string(), "@".to_string(), None));
                        }
                        
                        return Ok(Word::MapAccess(map_name.to_string(), key.to_string(), None));
                    }
                }
            }
            
            // Check for parameter expansion operators
            if braced_content.contains(":") {
                // Handle array slicing syntax like ${var:offset} or ${var:start:length}
                if let Some(colon_pos) = braced_content.find(':') {
                    let var_name = &braced_content[..colon_pos];
                    let slice_part = &braced_content[colon_pos + 1..];
                    
                    if let Some(second_colon) = slice_part.find(':') {
                        // This is ${var:start:length} syntax
                        let offset = &slice_part[..second_colon];
                        let length = &slice_part[second_colon + 1..];
                        return Ok(Word::ParameterExpansion(ParameterExpansion {
                            variable: var_name.to_string(),
                            operator: ParameterExpansionOperator::ArraySlice(offset.to_string(), Some(length.to_string())),
                            is_mutable: true,
                        }, None));
                    } else {
                        // This is ${var:offset} syntax
                        return Ok(Word::ParameterExpansion(ParameterExpansion {
                            variable: var_name.to_string(),
                            operator: ParameterExpansionOperator::ArraySlice(slice_part.to_string(), None),
                            is_mutable: true,
                        }, None));
                    }
                }
            }
            
            // Check if this is a parameter expansion with operators
            // Check longer patterns first to avoid partial matches
            if braced_content.ends_with("^^") {
                let base_var = braced_content.trim_end_matches("^^");
                Ok(Word::ParameterExpansion(ParameterExpansion {
                    variable: base_var.to_string(),
                    operator: ParameterExpansionOperator::UppercaseAll,
                    is_mutable: true,
                }, None))
            } else if braced_content.ends_with(",,") {
                let base_var = braced_content.trim_end_matches(",,");
                Ok(Word::ParameterExpansion(ParameterExpansion {
                    variable: base_var.to_string(),
                    operator: ParameterExpansionOperator::LowercaseAll,
                    is_mutable: true,
                }, None))
            } else if braced_content.ends_with("^") && !braced_content.ends_with("^^") {
                let base_var = braced_content.trim_end_matches("^");
                Ok(Word::ParameterExpansion(ParameterExpansion {
                    variable: base_var.to_string(),
                    operator: ParameterExpansionOperator::UppercaseFirst,
                    is_mutable: true,
                }, None))
            } else if braced_content.ends_with("##*/") {
                let base_var = braced_content.trim_end_matches("##*/");
                Ok(Word::ParameterExpansion(ParameterExpansion {
                    variable: base_var.to_string(),
                    operator: ParameterExpansionOperator::Basename,
                    is_mutable: true,
                }, None))
            } else if braced_content.ends_with("%/*") {
                let base_var = braced_content.trim_end_matches("%/*");
                Ok(Word::ParameterExpansion(ParameterExpansion {
                    variable: base_var.to_string(),
                    operator: ParameterExpansionOperator::Dirname,
                    is_mutable: true,
                }, None))
            } else if braced_content.contains("##") && !braced_content.ends_with("##*/") {
                let parts: Vec<&str> = braced_content.split("##").collect();
                if parts.len() == 2 {
                    Ok(Word::ParameterExpansion(ParameterExpansion {
                        variable: parts[0].to_string(),
                        operator: ParameterExpansionOperator::RemoveLongestPrefix(parts[1].to_string()),
                        is_mutable: true,
                    }, None))
                } else {
                    Ok(Word::Variable(braced_content, true, None))
                }
            } else if braced_content.contains("%%") && !braced_content.ends_with("%/*") {
                let parts: Vec<&str> = braced_content.split("%%").collect();
                if parts.len() == 2 {
                    Ok(Word::ParameterExpansion(ParameterExpansion {
                        variable: parts[0].to_string(),
                        operator: ParameterExpansionOperator::RemoveLongestSuffix(parts[1].to_string()),
                        is_mutable: true,
                    }, None))
                } else {
                    Ok(Word::Variable(braced_content, true, None))
                }
            } else if braced_content.contains("//") {
                let parts: Vec<&str> = braced_content.split("//").collect();
                if parts.len() == 3 {
                    Ok(Word::ParameterExpansion(ParameterExpansion {
                        variable: parts[0].to_string(),
                        operator: ParameterExpansionOperator::SubstituteAll(parts[1].to_string(), parts[2].to_string()),
                        is_mutable: true,
                    }, None))
                } else {
                    Ok(Word::Variable(braced_content, true, None))
                }
            } else if braced_content.contains("/") && !braced_content.contains("//") {
                let parts: Vec<&str> = braced_content.split("/").collect();
                if parts.len() == 3 {
                    Ok(Word::ParameterExpansion(ParameterExpansion {
                        variable: parts[0].to_string(),
                        operator: ParameterExpansionOperator::SubstituteAll(parts[1].to_string(), parts[2].to_string()),
                        is_mutable: true,
                    }, None))
                } else {
                    Ok(Word::Variable(braced_content, true, None))
                }
            } else {
                // If it's not a special case, return as a variable
                Ok(Word::Variable(braced_content, true, None))
            }
        }
        Some(Token::DollarParen) => {
            // Parse $(...) command substitution
            lexer.next();
            let command_text = lexer.capture_parenthetical_text()?;
            // For now, create a simple command as a placeholder
            // TODO: Parse the command_text into an actual Command
            let placeholder_cmd = Command::Simple(SimpleCommand {
                name: Word::Literal("echo".to_string(), None),
                args: vec![Word::Literal(command_text, None)],
                redirects: Vec::new(),
                env_vars: HashMap::new(),
                stdout_used: true,
                stderr_used: true,
            });
            Ok(Word::CommandSubstitution(Box::new(placeholder_cmd), None))
        }
        _ => {
            let (line, col) = lexer.offset_to_line_col(0);
            Err(ParserError::UnexpectedToken { token: Token::Identifier, line, col })
        }
    }
}

// Placeholder functions - these would need to be implemented based on the actual AST structures
fn parse_string_interpolation(lexer: &mut Lexer) -> Result<Word, ParserError> {
    use crate::ast::{StringInterpolation, StringPart};
    
    // Get the double-quoted string content (this includes the quotes)
    let string_content = lexer.get_string_text()?;
    
    // Remove the outer quotes
    let content = if string_content.starts_with('"') && string_content.ends_with('"') {
        &string_content[1..string_content.len()-1]
    } else {
        &string_content
    };
    
    // Parse the string content to extract literal parts and variable references
    let mut parts = Vec::new();
    let mut current_literal = String::new();
    let mut i = 0;
    
    while i < content.len() {
        if content[i..].starts_with("$") && i + 1 < content.len() {
            // We found a variable reference
            // First, add any accumulated literal text
            if !current_literal.is_empty() {
                parts.push(StringPart::Literal(current_literal.clone()));
                current_literal.clear();
            }
            
            // Check if this is a parameter expansion like ${var} or ${var[key]}
            if i + 1 < content.len() && content[i + 1..].starts_with('{') {
                // This is a parameter expansion ${...}
                i += 2; // skip $ and {
                let expansion_start = i;
                
                // Find the closing brace
                let mut brace_count = 1;
                while i < content.len() && brace_count > 0 {
                    match content[i..].chars().next() {
                        Some('{') => brace_count += 1,
                        Some('}') => brace_count -= 1,
                        _ => {}
                    }
                    i += 1;
                }
                
                if brace_count == 0 {
                    // We found a complete parameter expansion
                    let expansion_content = &content[expansion_start..i-1]; // -1 to exclude the closing }
                    
                    // Parse the parameter expansion content
                    if let Ok(expansion_word) = parse_parameter_expansion_content(expansion_content) {
                        parts.push(StringPart::ParameterExpansion(expansion_word));
                    } else {
                        // Fall back to treating it as a literal
                        parts.push(StringPart::Literal(format!("${{{}}}", expansion_content)));
                    }
                } else {
                    // Unmatched braces, treat as literal
                    parts.push(StringPart::Literal("${".to_string()));
                    i = expansion_start;
                }
            } else {
                // Simple variable reference like $var
                i += 1; // skip the $
                if i < content.len() {
                    let var_start = i;
                    
                    // Handle special shell variables like $#, $@, $*
                    if i < content.len() {
                        let next_char = content[i..].chars().next().unwrap();
                        if next_char == '#' || next_char == '@' || next_char == '*' {
                            // Special shell variable
                            parts.push(StringPart::Variable(next_char.to_string()));
                            i += 1;
                        } else if next_char.is_alphanumeric() || next_char == '_' {
                            // Regular variable name
                            while i < content.len() {
                                let next_char = content[i..].chars().next();
                                if let Some(c) = next_char {
                                    if c.is_alphanumeric() || c == '_' {
                                        i += 1;
                                    } else {
                                        break;
                                    }
                                } else {
                                    break;
                                }
                            }
                            let var_name = &content[var_start..i];
                            if !var_name.is_empty() {
                                parts.push(StringPart::Variable(var_name.to_string()));
                            }
                        }
                    }
                }
            }
        } else {
            // Add to current literal
            current_literal.push(content[i..].chars().next().unwrap());
            i += 1;
        }
    }
    
    // Add any remaining literal text
    if !current_literal.is_empty() {
        parts.push(StringPart::Literal(current_literal));
    }
    
    // If we have no parts, this shouldn't happen, but handle it gracefully
    if parts.is_empty() {
        parts.push(StringPart::Literal(content.to_string()));
    }
    
    Ok(Word::StringInterpolation(StringInterpolation { parts }, None))
}

fn parse_parameter_expansion_content(content: &str) -> Result<ParameterExpansion, ParserError> {
    // Parse parameter expansion content like "arr[1]", "map[foo]", "#arr[@]", etc.
    
    // Check for array length: #arr[@]
    if content.starts_with('#') && content.contains('[') && content.contains(']') {
        if let Some(bracket_start) = content.find('[') {
            if let Some(_bracket_end) = content.rfind(']') {
                // Keep the # prefix in the variable name so the generator can detect it
                let array_name = &content[..bracket_start]; // Keep # prefix
                return Ok(ParameterExpansion {
                    variable: array_name.to_string(),
                    operator: ParameterExpansionOperator::ArraySlice("@".to_string(), None),
                    is_mutable: true,
                });
            }
        }
    }
    
    // Check for map keys: !map[@]
    if content.starts_with('!') && content.contains('[') && content.contains(']') {
        if let Some(bracket_start) = content.find('[') {
            if let Some(_bracket_end) = content.rfind(']') {
                let map_name = &content[1..bracket_start]; // Remove ! prefix
                // This should return a Word::MapKeys, but we're in a ParameterExpansion context
                // so we mark it with a special operator that the generator can recognize
                return Ok(ParameterExpansion {
                    variable: format!("!{}", map_name), // Keep the ! prefix to indicate map keys
                    operator: ParameterExpansionOperator::ArraySlice("@".to_string(), None),
                    is_mutable: true,
                });
            }
        }
    }
    
    // Check for array/map access: arr[1], map[foo]
    if content.contains('[') && content.contains(']') {
        if let Some(bracket_start) = content.find('[') {
            if let Some(bracket_end) = content.rfind(']') {
                let var_name = &content[..bracket_start];
                let key = &content[bracket_start + 1..bracket_end];
                
                // Special case: if key is "@", this is array iteration
                if key == "@" {
                    return Ok(ParameterExpansion {
                        variable: var_name.to_string(),
                        operator: ParameterExpansionOperator::ArraySlice("@".to_string(), None),
                        is_mutable: true,
                    });
                }
                
                // This is array/map access - we'll handle this in the generator
                return Ok(ParameterExpansion {
                    variable: format!("{}[{}]", var_name, key),
                    operator: ParameterExpansionOperator::None,
                    is_mutable: true,
                });
            }
        }
    }
    
    // Check for parameter expansion operators
    // Check longer patterns first to avoid partial matches
    if content.ends_with("^^") {
        let base_var = content.trim_end_matches("^^");
        Ok(ParameterExpansion {
            variable: base_var.to_string(),
            operator: ParameterExpansionOperator::UppercaseAll,
            is_mutable: true,
        })
    } else if content.ends_with(",,") {
        let base_var = content.trim_end_matches(",,");
        Ok(ParameterExpansion {
            variable: base_var.to_string(),
            operator: ParameterExpansionOperator::LowercaseAll,
            is_mutable: true,
        })
    } else if content.ends_with("^") && !content.ends_with("^^") {
        let base_var = content.trim_end_matches("^");
        Ok(ParameterExpansion {
            variable: base_var.to_string(),
            operator: ParameterExpansionOperator::UppercaseFirst,
            is_mutable: true,
        })
    } else if content.ends_with("##*/") {
        let base_var = content.trim_end_matches("##*/");
        Ok(ParameterExpansion {
            variable: base_var.to_string(),
            operator: ParameterExpansionOperator::Basename,
            is_mutable: true,
        })
    } else if content.ends_with("%/*") {
        let base_var = content.trim_end_matches("%/*");
        Ok(ParameterExpansion {
            variable: base_var.to_string(),
            operator: ParameterExpansionOperator::Dirname,
            is_mutable: true,
        })
    } else if content.contains("##") && !content.ends_with("##*/") {
        let parts: Vec<&str> = content.split("##").collect();
        if parts.len() == 2 {
            Ok(ParameterExpansion {
                variable: parts[0].to_string(),
                operator: ParameterExpansionOperator::RemoveLongestPrefix(parts[1].to_string()),
                is_mutable: true,
            })
        } else {
            Ok(ParameterExpansion {
                variable: content.to_string(),
                operator: ParameterExpansionOperator::None,
                is_mutable: true,
            })
        }
    } else if content.contains("%%") && !content.ends_with("%/*") {
        let parts: Vec<&str> = content.split("%%").collect();
        if parts.len() == 2 {
            Ok(ParameterExpansion {
                variable: parts[0].to_string(),
                operator: ParameterExpansionOperator::RemoveLongestSuffix(parts[1].to_string()),
                is_mutable: true,
            })
        } else {
            Ok(ParameterExpansion {
                variable: content.to_string(),
                operator: ParameterExpansionOperator::None,
                is_mutable: true,
            })
        }
    } else if content.contains("#") && !content.starts_with('#') && !content.contains("##") {
        let parts: Vec<&str> = content.split("#").collect();
        if parts.len() == 2 {
            Ok(ParameterExpansion {
                variable: parts[0].to_string(),
                operator: ParameterExpansionOperator::RemoveShortestPrefix(parts[1].to_string()),
                is_mutable: true,
            })
        } else {
            Ok(ParameterExpansion {
                variable: content.to_string(),
                operator: ParameterExpansionOperator::None,
                is_mutable: true,
            })
        }
    } else if content.contains("%") && !content.contains("%%") && !content.ends_with("%/*") {
        let parts: Vec<&str> = content.split("%").collect();
        if parts.len() == 2 {
            Ok(ParameterExpansion {
                variable: parts[0].to_string(),
                operator: ParameterExpansionOperator::RemoveShortestSuffix(parts[1].to_string()),
                is_mutable: true,
            })
        } else {
            Ok(ParameterExpansion {
                variable: content.to_string(),
                operator: ParameterExpansionOperator::None,
                is_mutable: true,
            })
        }
    } else if content.contains("//") {
        let parts: Vec<&str> = content.split("//").collect();
        if parts.len() == 2 {
            // This is ${var//pattern/replacement} - split the second part by the first '/'
            let pattern_replacement = parts[1];
            if let Some(slash_pos) = pattern_replacement.find('/') {
                let pattern = &pattern_replacement[..slash_pos];
                let replacement = &pattern_replacement[slash_pos + 1..];
                Ok(ParameterExpansion {
                    variable: parts[0].to_string(),
                    operator: ParameterExpansionOperator::SubstituteAll(pattern.to_string(), replacement.to_string()),
                    is_mutable: true,
                })
            } else {
                Ok(ParameterExpansion {
                    variable: content.to_string(),
                    operator: ParameterExpansionOperator::None,
                    is_mutable: true,
                })
            }
        } else {
            Ok(ParameterExpansion {
                variable: content.to_string(),
                operator: ParameterExpansionOperator::None,
                is_mutable: true,
            })
        }
    } else if content.contains(":-") {
        let parts: Vec<&str> = content.split(":-").collect();
        if parts.len() == 2 {
            Ok(ParameterExpansion {
                variable: parts[0].to_string(),
                operator: ParameterExpansionOperator::DefaultValue(parts[1].to_string()),
                is_mutable: true,
            })
        } else {
            Ok(ParameterExpansion {
                variable: content.to_string(),
                operator: ParameterExpansionOperator::None,
                is_mutable: true,
            })
        }
    } else if content.contains(":=") {
        let parts: Vec<&str> = content.split(":=").collect();
        if parts.len() == 2 {
            Ok(ParameterExpansion {
                variable: parts[0].to_string(),
                operator: ParameterExpansionOperator::AssignDefault(parts[1].to_string()),
                is_mutable: true,
            })
        } else {
            Ok(ParameterExpansion {
                variable: content.to_string(),
                operator: ParameterExpansionOperator::None,
                is_mutable: true,
            })
        }
    } else if content.contains(":?") {
        let parts: Vec<&str> = content.split(":?").collect();
        if parts.len() == 2 {
            Ok(ParameterExpansion {
                variable: parts[0].to_string(),
                operator: ParameterExpansionOperator::ErrorIfUnset(parts[1].to_string()),
                is_mutable: true,
            })
        } else {
            Ok(ParameterExpansion {
                variable: content.to_string(),
                operator: ParameterExpansionOperator::None,
                is_mutable: true,
            })
        }
    } else {
        // Simple variable reference
        Ok(ParameterExpansion {
            variable: content.to_string(),
            operator: ParameterExpansionOperator::None,
            is_mutable: true,
        })
    }
}

fn parse_ansic_quoted_string(lexer: &mut Lexer) -> Result<Word, ParserError> {
    // Get the raw token text (e.g., "$'line1\nline2\tTabbed'")
    let raw_text = lexer.get_raw_token_text()?;
    
    // Extract the content between $' and ' (remove the $' prefix and ' suffix)
    if raw_text.len() < 3 || !raw_text.starts_with("$'") || !raw_text.ends_with("'") {
        return Err(ParserError::InvalidSyntax("Invalid ANSI-C quoted string format".to_string()));
    }
    
    let content = &raw_text[2..raw_text.len()-1]; // Remove $' and '
    
    // Process escape sequences
    let mut result = String::new();
    let mut chars = content.chars().peekable();
    
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next_ch) = chars.next() {
                match next_ch {
                    'a' => result.push('\x07'), // Bell
                    'b' => result.push('\x08'), // Backspace
                    'f' => result.push('\x0C'), // Form feed
                    'n' => result.push('\n'),   // Newline
                    'r' => result.push('\r'),   // Carriage return
                    't' => result.push('\t'),   // Tab
                    'v' => result.push('\x0B'), // Vertical tab
                    '\\' => result.push('\\'),  // Backslash
                    '\'' => result.push('\''),  // Single quote
                    '"' => result.push('"'),   // Double quote
                    '?' => result.push('?'),   // Question mark
                    '0' => result.push('\0'),  // Null byte
                    'x' => {
                        // Hex escape: \xHH
                        let mut hex_chars = String::new();
                        for _ in 0..2 {
                            if let Some(hex_ch) = chars.next() {
                                if hex_ch.is_ascii_hexdigit() {
                                    hex_chars.push(hex_ch);
                                } else {
                                    return Err(ParserError::InvalidSyntax(format!("Invalid hex escape: \\x{}", hex_ch)));
                                }
                            } else {
                                return Err(ParserError::InvalidSyntax("Incomplete hex escape".to_string()));
                            }
                        }
                        if let Ok(byte_val) = u8::from_str_radix(&hex_chars, 16) {
                            result.push(byte_val as char);
                        } else {
                            return Err(ParserError::InvalidSyntax(format!("Invalid hex value: {}", hex_chars)));
                        }
                    }
                    'u' => {
                        // Unicode escape: \uHHHH
                        let mut hex_chars = String::new();
                        for _ in 0..4 {
                            if let Some(hex_ch) = chars.next() {
                                if hex_ch.is_ascii_hexdigit() {
                                    hex_chars.push(hex_ch);
                                } else {
                                    return Err(ParserError::InvalidSyntax(format!("Invalid unicode escape: \\u{}", hex_ch)));
                                }
                            } else {
                                return Err(ParserError::InvalidSyntax("Incomplete unicode escape".to_string()));
                            }
                        }
                        if let Ok(unicode_val) = u32::from_str_radix(&hex_chars, 16) {
                            if let Some(unicode_char) = char::from_u32(unicode_val) {
                                result.push(unicode_char);
                            } else {
                                return Err(ParserError::InvalidSyntax(format!("Invalid unicode value: {}", unicode_val)));
                            }
                        } else {
                            return Err(ParserError::InvalidSyntax(format!("Invalid unicode hex value: {}", hex_chars)));
                        }
                    }
                    'U' => {
                        // Extended unicode escape: \UHHHHHHHH
                        let mut hex_chars = String::new();
                        for _ in 0..8 {
                            if let Some(hex_ch) = chars.next() {
                                if hex_ch.is_ascii_hexdigit() {
                                    hex_chars.push(hex_ch);
                                } else {
                                    return Err(ParserError::InvalidSyntax(format!("Invalid extended unicode escape: \\U{}", hex_ch)));
                                }
                            } else {
                                return Err(ParserError::InvalidSyntax("Incomplete extended unicode escape".to_string()));
                            }
                        }
                        if let Ok(unicode_val) = u32::from_str_radix(&hex_chars, 16) {
                            if let Some(unicode_char) = char::from_u32(unicode_val) {
                                result.push(unicode_char);
                            } else {
                                return Err(ParserError::InvalidSyntax(format!("Invalid extended unicode value: {}", unicode_val)));
                            }
                        } else {
                            return Err(ParserError::InvalidSyntax(format!("Invalid extended unicode hex value: {}", hex_chars)));
                        }
                    }
                    _ => {
                        // Unknown escape sequence, treat as literal
                        result.push('\\');
                        result.push(next_ch);
                    }
                }
            } else {
                // Backslash at end of string, treat as literal
                result.push('\\');
            }
        } else {
            result.push(ch);
        }
    }
    
    // Consume the token
    lexer.next();
    
    Ok(Word::Literal(result, None))
}

fn parse_brace_expansion(lexer: &mut Lexer) -> Result<Word, ParserError> {
    use crate::ast::{BraceExpansion, BraceItem, BraceRange};
    
    // Consume the opening brace
    if !matches!(lexer.peek(), Some(Token::BraceOpen)) {
        return Err(ParserError::InvalidSyntax("Expected '{' for brace expansion".to_string()));
    }
    lexer.next(); // consume '{'
    
    let mut items = Vec::new();
    
    // Parse the content inside braces
    loop {
        match lexer.peek() {
            Some(Token::BraceClose) => {
                lexer.next(); // consume '}'
                break;
            }
            Some(Token::Number) | Some(Token::PaddedNumber) => {
                let start = lexer.get_number_text()?;
//                 debug_eprintln!("DEBUG: Found start number: {}", start);
//                 debug_eprintln!("DEBUG: After getting start number, current token: {:?}", lexer.peek());
                
                // Check if this is a range (look for ..)
                if matches!(lexer.peek(), Some(Token::Range)) {
//                     debug_eprintln!("DEBUG: Found '..' after start number");
                    lexer.next(); // consume '..'
                    
                    if let Some(Token::Number) | Some(Token::PaddedNumber) = lexer.peek() {
                        let end = lexer.get_number_text()?;
//                         debug_eprintln!("DEBUG: Found end number: {}", end);
//                         debug_eprintln!("DEBUG: After getting end number, current token: {:?}", lexer.peek());
                        
                        // Check if there's a step value (another ..)
                        if matches!(lexer.peek(), Some(Token::Range)) {
//                             eprintln!("DEBUG: Found second '..' in number range, looking for step value");
                            lexer.next(); // consume second '..'
//                             eprintln!("DEBUG: After consuming second '..', current token: {:?}", lexer.peek());
                            
                            if let Some(Token::Number) | Some(Token::PaddedNumber) = lexer.peek() {
                                let step = lexer.get_number_text()?;
//                                 eprintln!("DEBUG: Found step value: {}", step);
//                                 eprintln!("DEBUG: Added step range, continuing to next iteration");
                                items.push(BraceItem::Range(BraceRange {
                                    start,
                                    end,
                                    step: Some(step),
                                    format: None,
                                }));
                                continue; // Continue to next iteration to look for closing brace or more items
                            } else {
//                                 eprintln!("DEBUG: Expected number after second '..', but got: {:?}", lexer.peek());
                                return Err(ParserError::InvalidSyntax("Expected number after second '..' in brace range".to_string()));
                            }
                        } else {
//                             eprintln!("DEBUG: No step value, creating range from {} to {}", start, end);
                            items.push(BraceItem::Range(BraceRange {
                                start,
                                end,
                                step: None,
                                format: None,
                            }));
//                             eprintln!("DEBUG: Added simple range, continuing to next iteration");
                            continue; // Continue to next iteration to look for closing brace or more items
                        }
                    } else {
//                         eprintln!("DEBUG: Expected number after '..', but got: {:?}", lexer.peek());
                        return Err(ParserError::InvalidSyntax("Expected number after '..' in brace range".to_string()));
                    }
                } else {
//                     eprintln!("DEBUG: No range, treating as literal number: {}", start);
                    // Just a literal number
                    items.push(BraceItem::Literal(start));
                }
            }
            Some(Token::Identifier) => {
                let text = lexer.get_identifier_text()?;
                
                // Check if this is a range (look for ..)
                if matches!(lexer.peek(), Some(Token::Range)) {
                    lexer.next(); // consume '..'
                    
                    if let Some(Token::Identifier) = lexer.peek() {
                        let end = lexer.get_identifier_text()?;
                        
                        // Check if there's a step value (another ..)
                        if matches!(lexer.peek(), Some(Token::Range)) {
                            lexer.next(); // consume second '..'
                            
                            if let Some(Token::Number) | Some(Token::PaddedNumber) = lexer.peek() {
                                let step = lexer.get_number_text()?;
                                items.push(BraceItem::Range(BraceRange {
                                    start: text,
                                    end,
                                    step: Some(step),
                                    format: None,
                                }));
                                continue; // Continue to next iteration to look for closing brace or more items
                            } else {
                                return Err(ParserError::InvalidSyntax("Expected number after second '..' in identifier brace range".to_string()));
                            }
                        } else {
                            items.push(BraceItem::Range(BraceRange {
                                start: text,
                                end,
                                step: None,
                                format: None,
                            }));
                            continue; // Continue to next iteration to look for closing brace or more items
                        }
                    } else {
                        return Err(ParserError::InvalidSyntax("Expected identifier after '..' in brace range".to_string()));
                    }
                } else {
                    // Just a literal identifier
                    items.push(BraceItem::Literal(text));
                }
            }
            Some(Token::Comma) => {
                lexer.next(); // consume ','
                // Continue to next item
            }
            _ => {
                return Err(ParserError::InvalidSyntax("Unexpected token in brace expansion".to_string()));
            }
        }
    }
    
    Ok(Word::BraceExpansion(BraceExpansion {
        prefix: None,
        items,
        suffix: None,
    }, None))
}

fn parse_arithmetic_expression(lexer: &mut Lexer) -> Result<Word, ParserError> {
    // Parse arithmetic expressions like $((i + 1))
    // First, consume the opening $(( or $(
    match lexer.peek() {
        Some(Token::Arithmetic) | Some(Token::ArithmeticEval) => {
            lexer.next(); // consume $(( or $(
        }
        _ => {
            return Err(ParserError::InvalidSyntax("Expected arithmetic expression start".to_string()));
        }
    }
    
    // Capture the content until we find the closing ))
    let mut expression_parts = Vec::new();
    let mut paren_depth = 1; // We're already inside one level of parentheses
    
    loop {
        match lexer.peek() {
            Some(Token::ArithmeticEvalClose) => {
                // This is the closing )) for $((...))
                lexer.next();
                paren_depth -= 1;
                if paren_depth == 0 {
                    break;
                }
            }
            Some(Token::Arithmetic) => {
                // This is another opening $((...))
                lexer.next();
                paren_depth += 1;
            }
            Some(Token::Identifier) => {
                expression_parts.push(lexer.get_identifier_text()?);
            }
            Some(Token::Number) => {
                expression_parts.push(lexer.get_number_text()?);
            }
            Some(Token::Plus) => {
                expression_parts.push("+".to_string());
                lexer.next();
            }
            Some(Token::Minus) => {
                expression_parts.push("-".to_string());
                lexer.next();
            }
            Some(Token::Star) => {
                expression_parts.push("*".to_string());
                lexer.next();
            }
            Some(Token::Slash) => {
                expression_parts.push("/".to_string());
                lexer.next();
            }
            Some(Token::Space) | Some(Token::Tab) => {
                expression_parts.push(" ".to_string());
                lexer.next();
            }
            Some(Token::Dollar) => {
                // Handle variable references like $i
                lexer.next();
                if let Some(Token::Identifier) = lexer.peek() {
                    let var_name = lexer.get_identifier_text()?;
                    expression_parts.push(format!("${}", var_name));
                } else {
                    return Err(ParserError::InvalidSyntax("Expected identifier after $ in arithmetic expression".to_string()));
                }
            }
            None => {
                return Err(ParserError::InvalidSyntax("Unexpected end of input in arithmetic expression".to_string()));
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
    Ok(Word::Arithmetic(ArithmeticExpression {
        expression,
        tokens: Vec::new(), // We don't need to store individual tokens for now
    }, None))
}

fn parse_braced_variable_name(lexer: &mut Lexer) -> Result<String, ParserError> {
    // Parse the content inside ${...} until we find the closing }
    let mut content = String::new();
    let mut brace_depth = 1; // We're already inside one level of braces
    
    while brace_depth > 0 {
        if let Some((start, end)) = lexer.get_span() {
            let token = lexer.peek();
            
            match token {
                Some(Token::BraceOpen) => {
                    brace_depth += 1;
                    let text = lexer.get_text(start, end);
                    content.push_str(&text);
                    lexer.next();
                }
                Some(Token::BraceClose) => {
                    brace_depth -= 1;
                    if brace_depth == 0 {
                        // Don't consume the closing } yet, let the caller handle it
                        break;
                    } else {
                        let text = lexer.get_text(start, end);
                        content.push_str(&text);
                        lexer.next();
                    }
                }
                _ => {
                    let text = lexer.get_text(start, end);
                    content.push_str(&text);
                    lexer.next();
                }
            }
        } else {
            break;
        }
    }
    
    Ok(content)
}

fn parse_parameter_expansion(_lexer: &mut Lexer) -> Result<Word, ParserError> {
    // TODO: Implement parameter expansion parsing
    Err(ParserError::InvalidSyntax("Parameter expansion not yet implemented".to_string()))
}

fn parse_array_slicing(_lexer: &mut Lexer, _array_name: String) -> Result<Word, ParserError> {
    // TODO: Implement array slicing parsing
    Err(ParserError::InvalidSyntax("Array slicing not yet implemented".to_string()))
}

fn parse_backtick_command_substitution(lexer: &mut Lexer) -> Result<Word, ParserError> {
    // Parse backtick command substitution
    let backtick_text = lexer.get_raw_token_text()?;
    // Remove the surrounding backticks
    let command_text = &backtick_text[1..backtick_text.len()-1];
    
    // Check if the command contains a pipeline (|)
    if command_text.contains('|') {
        // Parse as a pipeline
        let pipeline_parts: Vec<&str> = command_text.split('|').collect();
        let mut commands = Vec::new();
        for part in pipeline_parts {
            let command = parse_simple_command_from_text(part.trim())?;
            commands.push(command);
        }
        
        if commands.len() == 1 {
            Ok(Word::CommandSubstitution(Box::new(commands.remove(0)), None))
        } else {
            let pipeline = Command::Pipeline(Pipeline { commands, source_text: None, stdout_used: true, stderr_used: true });
            Ok(Word::CommandSubstitution(Box::new(pipeline), None))
        }
    } else {
        // Parse as a simple command (original logic)
        let parts: Vec<&str> = command_text.split_whitespace().collect();
        if parts.is_empty() {
            return Err(ParserError::InvalidSyntax("Empty command in backticks".to_string()));
        }
        
        let name = Word::Literal(parts[0].to_string(), None);
        let args: Vec<Word> = parts[1..].iter().map(|&s| Word::Literal(s.to_string(), None)).collect();
        
        let cmd = Command::Simple(SimpleCommand {
            name,
            args,
            redirects: vec![],
            env_vars: HashMap::new(),
            stdout_used: true,
            stderr_used: true,
        });
        
        Ok(Word::CommandSubstitution(Box::new(cmd), None))
    }
}

// Helper function to parse a simple command from text
fn parse_simple_command_from_text(text: &str) -> Result<Command, ParserError> {
    let cmd_parts: Vec<&str> = text.split_whitespace().collect();
    if cmd_parts.is_empty() {
        return Err(ParserError::InvalidSyntax("Empty command in process substitution".to_string()));
    }
    
    let name = Word::Literal(cmd_parts[0].to_string(), None);
    let args: Vec<Word> = cmd_parts[1..].iter().map(|&s| Word::Literal(s.to_string(), None)).collect();
    
    let cmd = Command::Simple(SimpleCommand {
        name,
        args,
        redirects: vec![],
        env_vars: HashMap::new(),
        stdout_used: true,
        stderr_used: true,
    });
    
    Ok(cmd)
}
