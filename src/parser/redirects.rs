use crate::ast::*;
use crate::lexer::{Lexer, Token};
use crate::parser::errors::ParserError;
use crate::parser::utilities::ParserUtilities;
use crate::parser::words::parse_word;
use std::collections::HashMap;

pub fn parse_redirect(lexer: &mut Lexer) -> Result<Redirect, ParserError> {
    let fd = if let Some(Token::Number) = lexer.peek() {
        let fd_str = lexer.get_number_text()?;
        Some(fd_str.parse().unwrap_or(0))
    } else {
        None
    };
    
    let operator = match lexer.next() {
        Some(Token::RedirectIn) => {
            if let Some(fd_num) = fd {
                if fd_num == 2 {
                    RedirectOperator::StderrInput
                } else {
                    RedirectOperator::Input
                }
            } else {
                RedirectOperator::Input
            }
        },
        Some(Token::RedirectOut) => {
            if let Some(fd_num) = fd {
                if fd_num == 2 {
                    RedirectOperator::StderrOutput
                } else {
                    RedirectOperator::Output
                }
            } else {
                RedirectOperator::Output
            }
        },
        Some(Token::RedirectAppend) => {
            if let Some(fd_num) = fd {
                if fd_num == 2 {
                    RedirectOperator::StderrAppend
                } else {
                    RedirectOperator::Append
                }
            } else {
                RedirectOperator::Append
            }
        },
        Some(Token::RedirectInOut) => RedirectOperator::Input, // Use Input as fallback
        Some(Token::Heredoc) => RedirectOperator::Heredoc,
        Some(Token::HeredocTabs) => RedirectOperator::HeredocTabs,
        Some(Token::HereString) => RedirectOperator::HereString,
        Some(Token::RedirectOutErr) => RedirectOperator::StderrOutput,
        Some(Token::RedirectInErr) => RedirectOperator::StderrInput,
        Some(Token::RedirectOutClobber) => RedirectOperator::Output, // Use Output as fallback
        Some(Token::RedirectAll) => RedirectOperator::Output, // Use Output as fallback
        Some(Token::RedirectAllAppend) => RedirectOperator::Append, // Use Append as fallback
        _ => return Err(ParserError::InvalidSyntax("Invalid redirect operator".to_string())),
    };
    
    // Here-string: '<<< word' often lexes as '<<' '<' then word; accept optional extra '<'
    if matches!(operator, RedirectOperator::Heredoc) {
        if let Some(Token::RedirectIn) = lexer.peek() { 
            lexer.next(); 
        }
    }
    
    // Skip whitespace before target
    lexer.skip_whitespace_and_comments();

    // Check for process substitution syntax: <(...)
    if matches!(operator, RedirectOperator::Input) && matches!(lexer.peek(), Some(Token::ParenOpen)) {
//         eprintln!("DEBUG: Found process substitution: <(...)");
        // This is a process substitution: <(...)
        let inner_text = lexer.capture_parenthetical_text()?;
//         eprintln!("DEBUG: Inner text: '{}'", inner_text);
        
        // Parse the inner command text to extract command name and arguments
        let inner_cmd = parse_command_from_text(lexer, &inner_text)?;
//         eprintln!("DEBUG: Parsed inner command: {:?}", inner_cmd);
        
        // Return a process substitution redirect
        return Ok(Redirect {
            fd,
            operator: RedirectOperator::ProcessSubstitutionInput(Box::new(inner_cmd)),
            target: Word::literal("".to_string()), // Not used for process substitution
            heredoc_body: None,
        });
    }
    
    // Check for process substitution with extra '<': < <(...)
    if matches!(operator, RedirectOperator::Input) && matches!(lexer.peek(), Some(Token::RedirectIn)) && matches!(lexer.peek_n(1), Some(Token::ParenOpen)) {
//         eprintln!("DEBUG: Found process substitution with extra <: < <(...)");
        // This is a process substitution: < <(...)
        lexer.next(); // consume the extra '<'
        let inner_text = lexer.capture_parenthetical_text()?;
//         eprintln!("DEBUG: Inner text: '{}'", inner_text);
        
        // Parse the inner command text to extract command name and arguments
        let inner_cmd = parse_command_from_text(lexer, &inner_text)?;
//         eprintln!("DEBUG: Parsed inner command: {:?}", inner_cmd);
        
        // Return a process substitution redirect
        return Ok(Redirect {
            fd,
            operator: RedirectOperator::ProcessSubstitutionInput(Box::new(inner_cmd)),
            target: Word::literal("".to_string()), // Not used for process substitution
            heredoc_body: None,
        });
    }

    // For here-strings, parse the string content as the target
    let target = if matches!(operator, RedirectOperator::HereString) {
        // For here-strings, parse the string content that follows
        parse_word(lexer)?
    } else {
        parse_word(lexer)?
    };
    
    // If this is a heredoc, capture lines until the delimiter is found at start of line
    // If this is a here-string, the target is the string content
    let heredoc_body = match operator {
        RedirectOperator::Heredoc | RedirectOperator::HeredocTabs => {
            parse_heredoc(lexer, &target)?
        }
        RedirectOperator::HereString => {
            // For here-strings, the target is the string content
            // We need to extract the string content from the target
            match &target {
                Word::Literal(s, _) => Some(s.clone()),
                Word::StringInterpolation(interp, _) => {
                    // For string interpolation, concatenate all parts
                    let mut content = String::new();
                    for part in &interp.parts {
                        match part {
                            StringPart::Literal(s) => content.push_str(&s),
                            _ => content.push_str(&format!("{:?}", part)), // Fallback for non-literal parts
                        }
                    }
                    Some(content)
                }
                _ => None,
            }
        }
        _ => None,
    };

    Ok(Redirect { fd, operator, target, heredoc_body })
}

fn parse_heredoc(lexer: &mut Lexer, target: &Word) -> Result<Option<String>, ParserError> {
    let delim = match target {
        Word::Literal(s, _) => s.clone(),
        _ => return Err(ParserError::InvalidSyntax("Heredoc delimiter must be a literal string".to_string())),
    };
    
//     eprintln!("DEBUG: parse_heredoc called with delimiter: '{}'", delim);
    
    // Skip to the next newline token
    while let Some(token) = lexer.peek() {
        match token {
            Token::Newline => {
                lexer.next(); // consume the newline
                break;
            }
            _ => {
                lexer.next(); // consume other tokens
            }
        }
    }

    // Get the current position in the input after the newline
    let start_pos = if let Some((start, _)) = lexer.get_span() {
        start
    } else {
        return Ok(Some(String::new()));
    };
    
//     eprintln!("DEBUG: Starting heredoc parsing from position: {}", start_pos);
    
    // Read the raw input line by line until we find the delimiter
    let mut body = String::new();
    let mut current_pos = start_pos;
    let input = &lexer.input;
    
    while current_pos < input.len() {
        // Find the end of the current line
        let line_end = input[current_pos..].find('\n').map(|i| current_pos + i).unwrap_or(input.len());
        let line = &input[current_pos..line_end];
        
//         eprintln!("DEBUG: Processing line: '{}'", line);
        
        // Check if this line is the delimiter (exact match, possibly with whitespace)
        if line.trim() == delim {
//             eprintln!("DEBUG: Found delimiter line, stopping");
            break;
        }
        
        // Add the line to the body
        body.push_str(line);
        
        // Add newline if there was one in the original input
        if line_end < input.len() && input.as_bytes()[line_end] == b'\n' {
            body.push('\n');
            current_pos = line_end + 1;
        } else {
            current_pos = line_end;
        }
    }
    
    // Advance the lexer to skip over the processed content
    // We need to consume tokens until we reach the delimiter
    while let Some(token) = lexer.peek() {
        match token {
            Token::Identifier => {
                let word = lexer.get_identifier_text()?;
                lexer.next();
                if word == delim {
                    break;
                }
            }
            _ => {
                lexer.next();
            }
        }
    }
    
//     eprintln!("DEBUG: Final heredoc body: '{}'", body);
    Ok(Some(body))
}

pub fn parse_process_substitution(lexer: &mut Lexer, is_input: bool) -> Result<Redirect, ParserError> {
    // Consume the opening < or >
    lexer.next();
    
    // Parse the inner command
    let inner = lexer.capture_parenthetical_text()?;
    
    // Parse the inner command
    let inner_cmd = parse_command_from_text(lexer, &inner)?;
    
    let operator = if is_input {
        RedirectOperator::ProcessSubstitutionInput(Box::new(inner_cmd))
    } else {
        RedirectOperator::ProcessSubstitutionOutput(Box::new(inner_cmd))
    };
    
    Ok(Redirect {
        fd: None,
        operator,
        target: Word::literal("".to_string()), // Not used for process substitution
        heredoc_body: None,
    })
}

// Parse command text into a Command AST node
fn parse_command_from_text(_lexer: &mut Lexer, text: &str) -> Result<Command, ParserError> {
    // Simple parsing of command text like "printf 'a\nb\n'" or "echo -e 'text' | sort"
    let trimmed = text.trim();
    
    // Check if it contains a pipeline
    if trimmed.contains('|') {
        let parts: Vec<&str> = trimmed.split('|').collect();
        let mut commands = Vec::new();
        for part in parts {
            let command = parse_simple_command_from_text(part.trim())?;
            commands.push(command);
        }
        
        if commands.len() == 1 {
            Ok(commands.remove(0))
        } else {
            let pipeline = Command::Pipeline(Pipeline { commands, source_text: None, stdout_used: true, stderr_used: true });
            Ok(pipeline)
        }
    } else {
        // Simple command without pipeline
        let cmd_parts: Vec<&str> = trimmed.split_whitespace().collect();
        if cmd_parts.is_empty() {
            return Err(ParserError::InvalidSyntax("Empty command in process substitution".to_string()));
        }
        
        let name = Word::literal(cmd_parts[0].to_string());
        let args: Vec<Word> = cmd_parts[1..].iter().map(|&s| Word::literal(s.to_string())).collect();
        
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
}

// Helper function to parse a simple command from text
fn parse_simple_command_from_text(text: &str) -> Result<Command, ParserError> {
    let cmd_parts: Vec<&str> = text.split_whitespace().collect();
    if cmd_parts.is_empty() {
        return Err(ParserError::InvalidSyntax("Empty command in process substitution".to_string()));
    }
    
    let name = Word::literal(cmd_parts[0].to_string());
    let args: Vec<Word> = cmd_parts[1..].iter().map(|&s| Word::literal(s.to_string())).collect();
    
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
