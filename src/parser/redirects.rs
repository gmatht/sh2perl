use crate::ast::*;
use crate::lexer::{Lexer, Token};
use crate::parser::errors::ParserError;
use crate::parser::utilities::ParserUtilities;
use crate::parser::words::parse_word;
use std::collections::{BTreeMap, HashMap};

/// Parse the redirect header (operator + target) but do NOT parse the heredoc
/// body.  Returns a partial Redirect; the caller must call
/// `parse_heredoc_body` for heredoc redirects after all redirects on the same
/// line have been collected.
pub fn parse_redirect_header(lexer: &mut Lexer) -> Result<Redirect, ParserError> {
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
        }
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
        }
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
        }
        Some(Token::RedirectInOut) => RedirectOperator::Input, // Use Input as fallback
        Some(Token::Heredoc) => RedirectOperator::Heredoc,
        Some(Token::HeredocTabs) => RedirectOperator::HeredocTabs,
        Some(Token::HereString) => RedirectOperator::HereString,
        Some(Token::RedirectOutErr) => RedirectOperator::StderrOutput,
        Some(Token::RedirectInErr) => RedirectOperator::StderrInput,
        Some(Token::RedirectOutClobber) => RedirectOperator::Output, // Use Output as fallback
        Some(Token::RedirectAll) => RedirectOperator::Output,        // Use Output as fallback
        Some(Token::RedirectAllAppend) => RedirectOperator::Append,  // Use Append as fallback
        _ => {
            return Err(ParserError::InvalidSyntax(
                "Invalid redirect operator".to_string(),
            ))
        }
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
    if matches!(operator, RedirectOperator::Input) && matches!(lexer.peek(), Some(Token::ParenOpen))
    {
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
            heredoc_quoted: false,
        });
    }

    // Check for process substitution with extra '<': < <(...)
    if matches!(operator, RedirectOperator::Input)
        && matches!(lexer.peek(), Some(Token::RedirectIn))
        && matches!(lexer.peek_n(1), Some(Token::ParenOpen))
    {
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
            heredoc_quoted: false,
        });
    }

    // For here-strings, parse the string content as the target
    let target = if matches!(operator, RedirectOperator::HereString) {
        // For here-strings, parse the string content that follows
        parse_word(lexer)?
    } else {
        parse_word(lexer)?
    };

    // If this is a heredoc, DO NOT parse the body here — the caller
    // (`parse_command_redirects`) will call `parse_heredoc_body` after
    // collecting all redirects on the same line.
    // For here-strings, extract static content as before.
    let heredoc_body = match operator {
        RedirectOperator::Heredoc | RedirectOperator::HeredocTabs => None,
        RedirectOperator::HereString => {
            // For here-strings, extract static content from the target.
            // If the here-string contains any dynamic parts (command substitution,
            // variable reference, parameter expansion), we cannot represent it as
            // a static string — return None so the generator will evaluate it at
            // runtime via word_to_perl.
            match &target {
                Word::Literal(s, _) => Some(s.clone()),
                Word::StringInterpolation(interp, _) => {
                    let mut content = String::new();
                    for part in &interp.parts {
                        match part {
                            StringPart::Literal(s) => content.push_str(&s),
                            _ => {
                                // Dynamic part — cannot be represented as a static string.
                                return Ok(Redirect {
                                    fd,
                                    operator,
                                    target,
                                    heredoc_body: None,
                                    heredoc_quoted: false,
                                });
                            }
                        }
                    }
                    Some(content)
                }
                _ => None,
            }
        }
        _ => None,
    };

    Ok(Redirect {
        fd,
        operator,
        target,
        heredoc_body,
        heredoc_quoted: false,
    })
}

/// Parse the body of a heredoc (or heredoc-tabs) redirect.
/// `target` must be the delimiter word.
/// Returns `(body, quoted)` where `quoted` indicates the delimiter was quoted (`<< 'EOF'`).
pub fn parse_heredoc_body(lexer: &mut Lexer, target: &Word) -> Result<(Option<String>, bool), ParserError> {
    // Determine if the delimiter was quoted by examining the raw input
    // at the delimiter position. The delimiter text is stored without quotes,
    // so we scan backwards from the heredoc body start to find the delimiter
    // in the raw input and check if it was quoted.
    let quoted = detect_heredoc_quoted(lexer, target);
    let body = parse_heredoc(lexer, target)?;
    Ok((body, quoted))
}

/// Full redirect parsing: header + heredoc body (if applicable).
pub fn parse_redirect(lexer: &mut Lexer) -> Result<Redirect, ParserError> {
    let header = parse_redirect_header(lexer)?;
    if matches!(&header.operator, RedirectOperator::Heredoc | RedirectOperator::HeredocTabs) {
        let (body, quoted) = parse_heredoc_body(lexer, &header.target)?;
        Ok(Redirect {
            heredoc_body: body,
            heredoc_quoted: quoted,
            ..header
        })
    } else {
        Ok(header)
    }
}

/// Detect whether the heredoc delimiter was quoted (`<< 'EOF'`) by examining
/// the raw input.  Scans backwards from the first newline after the current
/// position to find the delimiter and checks for surrounding quote characters.
fn detect_heredoc_quoted(lexer: &Lexer, target: &Word) -> bool {
    let delim = match target {
        Word::Literal(s, _) => s.clone(),
        _ => return false,
    };
    if delim.is_empty() {
        return false;
    }
    // Find the first newline after the current token position — this marks
    // the start of the heredoc body.  The delimiter must be before this newline.
    let start_pos = if let Some((cur_pos, _)) = lexer.get_span() {
        match lexer.input[cur_pos..].find('\n') {
            Some(nl_offset) => cur_pos + nl_offset,
            None => lexer.input.len(),
        }
    } else {
        return false;
    };
    // Search the entire line before start_pos for a quoted delimiter.
    let input = &lexer.input;
    // Find the beginning of the line containing the delimiter (search backwards from start_pos)
    let line_start = input[..start_pos].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line = &input[line_start..start_pos];
    // Look for 'delim' or "delim" (with quotes)
    let single_quoted = format!("'{}'", delim);
    let double_quoted = format!("\"{}\"", delim);
    line.contains(&single_quoted) || line.contains(&double_quoted)
}

fn parse_heredoc(lexer: &mut Lexer, target: &Word) -> Result<Option<String>, ParserError> {
    let delim = match target {
        Word::Literal(s, _) => s.clone(),
        _ => {
            return Err(ParserError::InvalidSyntax(
                "Heredoc delimiter must be a literal string".to_string(),
            ))
        }
    };

    // Find the start of the heredoc body in the raw input.
    // The body begins after the first newline following the command line.
    // We locate this newline from the raw input WITHOUT consuming any tokens,
    // because tokens before the newline may belong to a pipeline (e.g.
    // `cat << EOF | grep ... | sed ...`) and must remain in the token stream
    // for later pipeline parsing.
    let start_pos = if let Some((cur_pos, _)) = lexer.get_span() {
        match lexer.input[cur_pos..].find('\n') {
            Some(nl_offset) => cur_pos + nl_offset + 1,
            None => lexer.input.len(),
        }
    } else {
        return Ok(Some(String::new()));
    };

    // Save the current lexer position; we will only advance past tokens
    // that lie within the heredoc body (i.e. at or after start_pos).
    let saved_lexer_current = lexer.current;

    // Read the raw input line by line until we find the delimiter
    let mut body = String::new();
    let mut current_pos = start_pos;
    let input = &lexer.input;

    while current_pos < input.len() {
        // Find the end of the current line
        let line_end = input[current_pos..]
            .find('\n')
            .map(|i| current_pos + i)
            .unwrap_or(input.len());
        let line = &input[current_pos..line_end];

        // Check if this line is the delimiter (exact match, possibly with whitespace)
        if line.trim() == delim {
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

    // Compute the byte position right after the delimiter line.
    // current_pos is at the start of the delimiter; find the end of this line.
    let body_end = if let Some(nl_pos) = input[current_pos..].find('\n') {
        // Include the newline after the delimiter.
        current_pos + nl_pos + 1
    } else {
        // No newline after delimiter — end of input.
        input.len()
    };

    // Remove the heredoc body tokens from the token stream entirely.
    // Tokens before start_pos (pipeline operators like `|`, `grep`, etc.)
    // must remain for later pipeline parsing, so we only remove the range
    // [body_start_idx, body_end_idx) of tokens whose start >= start_pos.
    let mut body_start_idx = saved_lexer_current;
    while body_start_idx < lexer.tokens.len() {
        if lexer.tokens[body_start_idx].1 >= start_pos {
            break;
        }
        body_start_idx += 1;
    }
    let mut body_end_idx = body_start_idx;
    while body_end_idx < lexer.tokens.len() {
        if lexer.tokens[body_end_idx].1 >= body_end {
            break;
        }
        body_end_idx += 1;
    }
    // Remove the body tokens.  lexer.current is unchanged (still pointing at
    // pipeline tokens before start_pos), so pipeline parsing can proceed.
    if body_end_idx > body_start_idx {
        lexer.tokens.drain(body_start_idx..body_end_idx);
    }

    //     eprintln!("DEBUG: Final heredoc body: '{}'", body);
    Ok(Some(body))
}

pub fn parse_process_substitution(
    lexer: &mut Lexer,
    is_input: bool,
) -> Result<Redirect, ParserError> {
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
        heredoc_quoted: false,
    })
}

// Parse command text into a Command AST node
fn parse_command_from_text(_lexer: &mut Lexer, text: &str) -> Result<Command, ParserError> {
    let trimmed = text.trim();
    let mut parser = crate::parser::commands::Parser::new(trimmed);
    let commands = parser.parse()?;

    if commands.len() == 1 {
        Ok(commands[0].clone())
    } else if commands.is_empty() {
        Err(ParserError::InvalidSyntax(
            "Empty command in process substitution".to_string(),
        ))
    } else {
        Ok(Command::Pipeline(Pipeline {
            commands,
            source_text: None,
            stdout_used: true,
            stderr_used: true,
        }))
    }
}
