use crate::ast::*;
use crate::lexer::{Lexer, Token};
use crate::parser::errors::ParserError;
use crate::parser::utilities::ParserUtilities;
use crate::parser::words::parse_word;
use logos::Logos;
use std::collections::{BTreeMap, HashMap};

/// Parse the redirect header (operator + target) but do NOT parse the heredoc
/// body.  Returns a partial Redirect; the caller must call
/// `parse_heredoc_body` for heredoc redirects after all redirects on the same
/// line have been collected.
pub fn parse_redirect_header(lexer: &mut Lexer) -> Result<Redirect, ParserError> {
    if crate::debug::is_debug_enabled() {
        eprintln!("DEBUG parse_redirect_header: peek={:?}", lexer.peek());
    }
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
        Some(Token::RedirectOutClobber) => RedirectOperator::ClobberOutput,
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

    // Check for process substitution output syntax: >(...)
    // This occurs after a redirect like `exec 1> >(tee ...)` where the `>`
    // is the redirect and `>(...)` is the process substitution target.
    if matches!(
        operator,
        RedirectOperator::Output | RedirectOperator::StderrOutput
    ) && matches!(lexer.peek(), Some(Token::RedirectOut))
        && matches!(lexer.peek_n(1), Some(Token::ParenOpen))
    {
        lexer.next(); // consume the extra '>' (start of >(...))
        let inner_text = lexer.capture_parenthetical_text()?;

        // Parse the inner command text to extract command name and arguments
        let inner_cmd = parse_command_from_text(lexer, &inner_text)?;

        // Return a process substitution redirect
        return Ok(Redirect {
            fd,
            operator: RedirectOperator::ProcessSubstitutionOutput(Box::new(inner_cmd)),
            target: Word::literal("".to_string()), // Not used for process substitution
            heredoc_body: None,
            heredoc_quoted: false,
        });
    }

    // For here-strings, parse the string content as the target
    let target_raw = if matches!(operator, RedirectOperator::HereString) {
        // For here-strings, parse the string content that follows
        parse_word(lexer)?
    } else {
        parse_word(lexer)?
    };

    // For heredocs, strip leading backslash from the delimiter.
    // `<<\EOF` means the delimiter is `EOF` (backslash quotes it).
    // Also handle the case where logos created a SingleQuotedString that
    // spans past the heredoc delimiter into the body (e.g. `<< 'EOF'`
    // where the body contains an apostrophe `it's`). In that case,
    // truncate at the first newline.
    let mut heredoc_quoted = false;
    let target = match operator {
        RedirectOperator::Heredoc | RedirectOperator::HeredocTabs => {
            if let Word::Literal(s, meta) = &target_raw {
                // Truncate at first newline if the SingleQuotedString swallowed
                // part of the heredoc body (logos doesn't understand heredocs).
                let truncated = if let Some(nl_pos) = s.find('\n') {
                    s[..nl_pos].to_string()
                } else {
                    s.clone()
                };
                // Check for backslash-quoted delimiter: `<<\\EOF` → `EOF`.
                if truncated.starts_with('\\') {
                    heredoc_quoted = true;
                    if crate::debug::is_debug_enabled() {
                        eprintln!(
                            "DEBUG: stripped backslash from heredoc delimiter '{}' -> '{}'",
                            truncated,
                            &truncated[1..]
                        );
                    }
                    Word::Literal(truncated[1..].to_string(), *meta)
                // Check for single-quoted delimiter: `<<'EOF'` → `EOF`.
                // The SingleQuotedString token consumed the quotes, so the
                // truncated text may be `'EOF'` (quotes included, if the token
                // boundary happened to be at the delimiter's closing quote), or
                // `EOF'` (trailing quote only, if the inner `'` was consumed as
                // a regular character of the over-greedy SQ string and the outer
                // opening quote was already stripped by parse_word).
                } else if truncated.len() >= 2
                    && truncated.starts_with('\'')
                    && truncated.ends_with('\'')
                {
                    heredoc_quoted = true;
                    if crate::debug::is_debug_enabled() {
                        eprintln!(
                            "DEBUG: stripped single quotes from heredoc delimiter '{}' -> '{}'",
                            truncated,
                            &truncated[1..truncated.len() - 1]
                        );
                    }
                    Word::Literal(truncated[1..truncated.len() - 1].to_string(), *meta)
                // Check for double-quoted delimiter: `<<"EOF"` → `EOF`.
                } else if truncated.len() >= 2
                    && truncated.starts_with('"')
                    && truncated.ends_with('"')
                {
                    heredoc_quoted = true;
                    if crate::debug::is_debug_enabled() {
                        eprintln!(
                            "DEBUG: stripped double quotes from heredoc delimiter '{}' -> '{}'",
                            truncated,
                            &truncated[1..truncated.len() - 1]
                        );
                    }
                    Word::Literal(truncated[1..truncated.len() - 1].to_string(), *meta)
                // Check for trailing single-quote after delimiter name.
                // This happens when logos consumed the delimiter's closing `'`
                // as a regular character of an over-greedy SingleQuotedString
                // token, and parse_word stripped only the outer quotes leaving
                // the inner `'` at the end of the truncated text.
                // Example: `<< 'EOF'` where the SQ token spans
                // `'EOF'\nThis is a test with an apostrophe: it'`.
                // After outer-quote stripping and newline truncation we get
                // `EOF'`.  Strip the trailing `'` and mark as quoted.
                } else if truncated.ends_with('\'') || truncated.ends_with('"') {
                    let quote_char = if truncated.ends_with('\'') { '\'' } else { '"' };
                    heredoc_quoted = true;
                    let clean = truncated[..truncated.len() - 1].to_string();
                    if crate::debug::is_debug_enabled() {
                        eprintln!(
                            "DEBUG: stripped trailing quote from heredoc delimiter '{}' -> '{}'",
                            truncated, clean
                        );
                    }
                    Word::Literal(clean, *meta)
                } else {
                    Word::Literal(truncated, *meta)
                }
            } else {
                target_raw
            }
        }
        _ => target_raw,
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
                                    heredoc_quoted,
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
        heredoc_quoted,
    })
}

/// Parse the body of a heredoc (or heredoc-tabs) redirect.
/// `target` must be the delimiter word.
/// Returns `(body, quoted)` where `quoted` indicates the delimiter was quoted (`<< 'EOF'`).
pub fn parse_heredoc_body(
    lexer: &mut Lexer,
    target: &Word,
    strip_tabs: bool,
) -> Result<(Option<String>, bool), ParserError> {
    // Determine if the delimiter was quoted by examining the raw input
    // at the delimiter position. The delimiter text is stored without quotes,
    // so we scan backwards from the heredoc body start to find the delimiter
    // in the raw input and check if it was quoted.
    let quoted = detect_heredoc_quoted(lexer, target);
    let body = parse_heredoc(lexer, target, strip_tabs)?;
    Ok((body, quoted))
}

/// Full redirect parsing: header + heredoc body (if applicable).
pub fn parse_redirect(lexer: &mut Lexer) -> Result<Redirect, ParserError> {
    if crate::debug::is_debug_enabled() {
        eprintln!(
            "DEBUG parse_redirect called, lexer.current={}",
            lexer.current
        );
    }
    let header = parse_redirect_header(lexer)?;
    if matches!(
        &header.operator,
        RedirectOperator::Heredoc | RedirectOperator::HeredocTabs
    ) {
        let strip_tabs = header.operator == RedirectOperator::HeredocTabs;
        let (body, quoted) = parse_heredoc_body(lexer, &header.target, strip_tabs)?;
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
/// the raw input.  Scans backwards from the current lexer position to find
/// the Heredoc token, then examines the token line for quoted delimiters.
/// This approach works correctly even when logos created a SingleQuotedString
/// that spanned past the heredoc delimiter (e.g. `<< 'EOF'` where the body
/// contains an apostrophe).
fn detect_heredoc_quoted(lexer: &Lexer, target: &Word) -> bool {
    let delim = match heredoc_delim_from_word(target) {
        Some(s) => s,
        None => return false,
    };
    if delim.is_empty() {
        return false;
    }
    let input = &lexer.input;
    // Scan backwards from the current lexer position to find the Heredoc token.
    let current = lexer.current;
    let mut heredoc_end = None;
    let mut scan_idx = current.saturating_sub(1);
    loop {
        if let Some(tok) = lexer.tokens.get(scan_idx) {
            if matches!(tok.0, Token::Heredoc | Token::HeredocTabs) {
                heredoc_end = Some(tok.2);
                break;
            }
            if scan_idx == 0 {
                break;
            }
            scan_idx -= 1;
        } else {
            break;
        }
    }
    // Find the first newline after the heredoc token
    let start_pos = match heredoc_end {
        Some(end) => match input[end..].find('\n') {
            Some(nl_offset) => end + nl_offset,
            None => input.len(),
        },
        None => {
            // Fallback: use current token position
            if let Some((cur_pos, _)) = lexer.get_span() {
                match input[cur_pos..].find('\n') {
                    Some(nl_offset) => cur_pos + nl_offset,
                    None => input.len(),
                }
            } else {
                return false;
            }
        }
    };
    // Search the entire line before start_pos for a quoted delimiter.
    // Find the beginning of the line containing the delimiter (search backwards from start_pos)
    let line_start = input[..start_pos].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line = &input[line_start..start_pos];
    // Look for 'delim' or "delim" (with quotes) or \delim (backslash-quoted)
    let single_quoted = format!("'{}'", delim);
    let double_quoted = format!("\"{}\"", delim);
    let backslash_quoted = format!("\\{}", delim);
    line.contains(&single_quoted)
        || line.contains(&double_quoted)
        || line.contains(&backslash_quoted)
}

/// Helper: extract a literal string from a Word, accepting both
/// `Literal` and `StringInterpolation` with a single literal part.
fn heredoc_delim_from_word(word: &Word) -> Option<String> {
    match word {
        Word::Literal(s, _) => Some(s.clone()),
        Word::StringInterpolation(interp, _) => {
            if interp.parts.len() == 1 {
                if let crate::ast_words::StringPart::Literal(s) = &interp.parts[0] {
                    return Some(s.clone());
                }
            }
            None
        }
        _ => None,
    }
}

fn parse_heredoc(
    lexer: &mut Lexer,
    target: &Word,
    strip_tabs: bool,
) -> Result<Option<String>, ParserError> {
    let delim = match heredoc_delim_from_word(target) {
        Some(s) => s,
        None => {
            return Err(ParserError::InvalidSyntax(
                "Heredoc delimiter must be a literal string".to_string(),
            ))
        }
    };
    if crate::debug::is_debug_enabled() {
        eprintln!("DEBUG parse_heredoc: delim='{}'", delim);
    }

    // Find the start of the heredoc body in the raw input.
    // We scan backwards from the current lexer position to find the
    // Heredoc token, then find the first newline after its end.
    // This approach works even when logos created a SingleQuotedString
    // that spanned past the heredoc delimiter (e.g. `<< 'EOF'` where
    // the body contains an apostrophe).
    let saved_lexer_current = lexer.current;
    let start_pos = {
        let mut body_start = None;
        // Scan backwards through tokens to find the Heredoc/HeredocTabs token
        let mut scan_idx = saved_lexer_current.saturating_sub(1);
        loop {
            if let Some(tok) = lexer.tokens.get(scan_idx) {
                if matches!(tok.0, Token::Heredoc | Token::HeredocTabs) {
                    // Found the heredoc operator. The body starts after the
                    // first newline following this token.
                    if let Some(nl_offset) = lexer.input[tok.2..].find('\n') {
                        body_start = Some(tok.2 + nl_offset + 1);
                    } else {
                        body_start = Some(lexer.input.len());
                    }
                    break;
                }
                if scan_idx == 0 {
                    break;
                }
                scan_idx -= 1;
            } else {
                break;
            }
        }
        // Fallback: use the current token position
        body_start.unwrap_or_else(|| {
            if let Some((cur_pos, _)) = lexer.get_span() {
                match lexer.input[cur_pos..].find('\n') {
                    Some(nl_offset) => cur_pos + nl_offset + 1,
                    None => lexer.input.len(),
                }
            } else {
                lexer.input.len()
            }
        })
    };
    if crate::debug::is_debug_enabled() {
        eprintln!(
            "DEBUG parse_heredoc: start_pos={}, input[..]={:?}",
            start_pos,
            &lexer.input[start_pos..start_pos + 40.min(lexer.input.len() - start_pos)]
        );
    }

    // Read the raw input line by line until we find the delimiter
    let mut body = String::new();
    let mut current_pos = start_pos;
    let input = &lexer.input;

    while current_pos < input.len() {
        let line_end = input[current_pos..]
            .find('\n')
            .map(|i| current_pos + i)
            .unwrap_or(input.len());
        let line = &input[current_pos..line_end];
        if line.trim() == delim {
            break;
        }
        // For <<- heredocs, strip leading tab characters from each line
        if strip_tabs {
            let stripped = line.trim_start_matches('\t');
            body.push_str(stripped);
        } else {
            body.push_str(line);
        }
        if line_end < input.len() && input.as_bytes()[line_end] == b'\n' {
            body.push('\n');
            current_pos = line_end + 1;
        } else {
            current_pos = line_end;
        }
    }

    // Compute the byte position right after the delimiter line.
    let body_end = if let Some(nl_pos) = input[current_pos..].find('\n') {
        current_pos + nl_pos + 1
    } else {
        input.len()
    };

    // --- Fix: properly handle tokens that span across heredoc boundaries ---
    //
    // Logos tokenizes the entire input without understanding heredocs, so a `'`
    // inside the heredoc body can start a SingleQuotedString that swallows
    // content past the heredoc delimiter.  We remove all body tokens and then
    // re-tokenize any content after body_end that was consumed by spanning tokens.

    // Step 1: truncate tokens that start before start_pos but end after it.
    let mut scan_back = saved_lexer_current.saturating_sub(1);
    loop {
        if let Some(tok) = lexer.tokens.get(scan_back) {
            if tok.2 > start_pos && tok.1 < start_pos {
                lexer.tokens[scan_back].2 = start_pos;
            }
            if scan_back == 0 {
                break;
            }
            scan_back -= 1;
        } else {
            break;
        }
    }

    // Step 2: find first token with start >= start_pos.
    let mut body_start_idx = saved_lexer_current;
    while body_start_idx < lexer.tokens.len() {
        if lexer.tokens[body_start_idx].1 >= start_pos {
            break;
        }
        body_start_idx += 1;
    }

    // Step 3: scan tokens until we find one with start >= body_end.
    let mut span_end = body_start_idx;
    while span_end < lexer.tokens.len() {
        if lexer.tokens[span_end].1 >= body_end {
            break;
        }
        span_end += 1;
    }

    // The next surviving token starts at body_end or later.
    let mut next_start = if span_end < lexer.tokens.len() {
        lexer.tokens[span_end].1
    } else {
        input.len()
    };

    // If the next surviving token is far beyond the heredoc body, we likely had
    // spanning SingleQuotedStrings that consumed part of the file.
    // Extend next_start to the end of the input so the entire remainder
    // is re-tokenized correctly.
    // Use a threshold of 16 bytes to avoid triggering on small gaps.
    if next_start > body_end + 16 && next_start < input.len() {
        next_start = input.len();
    }
    // Also extend next_start past any tokens that start in [body_end, next_start)
    // but end past next_start (spanning tokens from the body).
    for i in span_end..lexer.tokens.len() {
        let (tok_start, tok_end) = (lexer.tokens[i].1, lexer.tokens[i].2);
        if tok_start >= next_start {
            break;
        }
        if tok_start >= body_end && tok_end > next_start {
            next_start = tok_end;
            break;
        }
    }

    // Remove all tokens in [body_start_idx, span_end) and also any old tokens
    // that start in [body_end, next_start).  These were created by the original
    // logos run and will be replaced by the re-tokenized gap below.
    let mut remove_end = span_end;
    while remove_end < lexer.tokens.len() {
        if lexer.tokens[remove_end].1 >= next_start {
            break;
        }
        remove_end += 1;
    }
    if lexer.current >= body_start_idx && lexer.current < remove_end {
        lexer.current = saved_lexer_current;
    }
    let remove_len = remove_end - body_start_idx;
    if remove_len > 0 {
        lexer.tokens.drain(body_start_idx..remove_end);
        // Set current back to saved_lexer_current so the caller
        // (e.g. parse_command_redirects) can continue processing
        // any additional redirects on the same line (e.g. `2>&1 >/dev/null`
        // after `<<EOF`).  The body tokens have been removed, but any
        // redirect tokens that appeared after the heredoc delimiter on
        // the same line are still in the token list before body_start_idx.
        if lexer.current >= body_start_idx {
            lexer.current = saved_lexer_current;
        }
        // After drain, ensure lexer.current is not past the end
        if lexer.current >= lexer.tokens.len() && !lexer.tokens.is_empty() {
            lexer.current = lexer.tokens.len() - 1;
        }
    }

    // Step 4: re-tokenize gap between body_end and next_start.
    // This content was consumed by spanning tokens (e.g. SingleQuotedStrings
    // that started inside the body).  Using logos to re-tokenize works here
    // because logos sees the standalone content without the preceding `'` that
    // originally caused the spanning.
    if next_start > body_end {
        let gap_text = &input[body_end..next_start];
        if !gap_text.is_empty() && gap_text.bytes().any(|b| !b.is_ascii_whitespace()) {
            let mut gap_lexer = Token::lexer(gap_text);
            let mut gap_tokens: Vec<(Token, usize, usize)> = Vec::new();
            while let Some(token_result) = gap_lexer.next() {
                let span = gap_lexer.span();
                match token_result {
                    Ok(tok) => {
                        gap_tokens.push((tok, body_end + span.start, body_end + span.end));
                    }
                    Err(_) => continue,
                }
            }
            // Insert the re-tokenized gap content.
            let insert_at = body_start_idx;
            for (j, gt) in gap_tokens.iter().enumerate() {
                lexer.tokens.insert(insert_at + j, gt.clone());
            }
            // Point lexer.current back to saved_lexer_current so the
            // caller can continue processing any additional redirects
            // on the same line.  The re-tokenized gap tokens are now
            // in place after body_start_idx and will be encountered
            // after the newline separator.
            lexer.current = saved_lexer_current;
            // Re-apply DoubleQuotedString merging to the re-tokenized content
            // because logos re-tokenization does not handle nesting of
            // $(...), ${...}, and backtick command substitutions inside DQS.
            Lexer::merge_double_quoted_strings(&lexer.input, &mut lexer.tokens);
            // Apply the same post-processing steps as Lexer::new to the
            // modified token list: split over-greedy single-quoted strings,
            // fix bare quotes that logos failed to pair, and fix split comments.
            // These steps are necessary because the re-tokenized gap may contain
            // single quotes that were previously part of spanning SQS tokens,
            // and the raw logos output for the gap may not have gone through
            // the full post-processing pipeline.
            Lexer::split_overgreedy_sq(&lexer.input, &mut lexer.tokens);
            Lexer::fix_split_comments(&lexer.input, &mut lexer.tokens);
            Lexer::fix_bare_quotes(&lexer.input, &mut lexer.tokens);
            // After fix_bare_quotes may have created new SQS tokens that overlap
            // with existing ones, run split_overgreedy_sq again to fix them.
            Lexer::split_overgreedy_sq(&lexer.input, &mut lexer.tokens);
        }
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
