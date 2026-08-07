use crate::lexer::{Lexer, Token};
use crate::parser::errors::ParserError;

pub trait ParserUtilities {
    fn skip_whitespace_and_comments(&mut self);
    fn skip_inline_whitespace_and_comments(&mut self);
    fn capture_parenthetical_text(&mut self) -> Result<String, ParserError>;
    fn capture_double_bracket_expression(&mut self) -> Result<String, ParserError>;
    fn capture_single_bracket_expression(&mut self) -> Result<String, ParserError>;
    fn get_identifier_text(&mut self) -> Result<String, ParserError>;
    fn get_number_text(&mut self) -> Result<String, ParserError>;
    fn get_raw_token_text(&mut self) -> Result<String, ParserError>;
    fn get_string_text(&mut self) -> Result<String, ParserError>;
    fn get_current_text(&mut self) -> Option<String>;
    fn get_text(&mut self, start: usize, end: usize) -> String;
    fn get_span(&mut self) -> Option<(usize, usize)>;
    fn current_position(&mut self) -> usize;
    fn offset_to_line_col(&mut self, offset: usize) -> (usize, usize);
    fn peek(&mut self) -> Option<Token>;
    fn peek_n(&mut self, n: usize) -> Option<Token>;
    fn next(&mut self) -> Option<Token>;
    fn is_eof(&mut self) -> bool;
    fn consume(&mut self, expected: Token) -> Result<(), ParserError>;
}

impl ParserUtilities for Lexer {
    fn get_text(&mut self, start: usize, end: usize) -> String {
        self.input[start..end].to_string()
    }

    fn get_span(&mut self) -> Option<(usize, usize)> {
        self.tokens
            .get(self.current)
            .map(|(_, start, end)| (*start, *end))
    }

    fn skip_whitespace_and_comments(&mut self) {
        while let Some(token) = self.peek() {
            match token {
                Token::Space
                | Token::Tab
                | Token::Comment
                | Token::Newline
                | Token::CarriageReturn => {
                    self.next();
                }
                _ => break,
            }
        }
    }

    fn skip_inline_whitespace_and_comments(&mut self) {
        while let Some(token) = self.peek() {
            match token {
                Token::Space | Token::Tab | Token::Comment => {
                    self.next();
                }
                _ => break,
            }
        }
    }

    fn capture_parenthetical_text(&mut self) -> Result<String, ParserError> {
        let mut content = String::new();
        let mut depth = 1;
        let start_pos = self.current;

        // Consume the opening parenthesis
        self.next();

        while depth > 0 {
            match self.peek() {
                Some(Token::ParenOpen) => {
                    depth += 1;
                    content.push('(');
                    self.next();
                }
                Some(Token::ParenClose) => {
                    depth -= 1;
                    if depth > 0 {
                        content.push(')');
                    }
                    self.next();
                }
                Some(Token::ArithmeticEvalClose) => {
                    // `))` closes two levels of paren depth
                    // (ArithmeticEvalClose has priority over ParenClose via logos).
                    // When depth == 1 the FIRST `)` closes THIS $(...); the
                    // second belongs to an ENCLOSING context — the classic
                    // shape is a cmdsub as the last element of an array
                    // literal (`arr=($(cmd))` — the lexer merges the cmdsub's
                    // `)` with the array's `)` into one token). Pushing
                    // anything (the old code pushed "))") corrupted the
                    // captured text and broke the re-parse. The enclosing
                    // parser detects the swallowed closer via the last-token
                    // check in parse_array_elements.
                    if depth >= 3 {
                        depth -= 2;
                        content.push_str("))");
                    } else if depth == 2 {
                        depth = 0;
                        content.push(')'); // one ) is inside, one closes
                    } else {
                        // depth == 1: first ) closes us, second is outside
                        depth = 0;
                    }
                    self.next();
                }
                Some(Token::ArithmeticEval) => {
                    // ((...)) adds two opening parens.
                    depth += 2;
                    let text = self.get_current_text().unwrap_or_default();
                    content.push_str(&text);
                    self.next();
                }
                Some(Token::Arithmetic) => {
                    // $((...)) adds two opening parens
                    depth += 2;
                    let text = self.get_current_text().unwrap_or_default();
                    content.push_str(&text);
                    self.next();
                }
                Some(Token::DollarParen) => {
                    // Nested $(...) - the ( is already consumed by the $ token,
                    // but the matching ) will be a ParenClose, so increase depth
                    depth += 1;
                    let text = self.get_current_text().unwrap_or_default();
                    content.push_str(&text);
                    self.next();
                }
                Some(Token::Comment) => {
                    // A Comment token may contain `)` characters when `#` appears
                    // inside ${...} parameter-expansion operators (e.g. ${var#pattern}
                    // or ${var##pattern}) inside $(...).  The logos lexer treats `#...`
                    // as a line comment, which swallows the closing `)` of the $(...).
                    // Scan the comment text for `)` and adjust depth accordingly.
                    let idx = self.current;
                    let cm_start = self.tokens[idx].1;
                    let cm_end = self.tokens[idx].2;
                    let text = self.input[cm_start..cm_end].to_string();

                    // Find the first `)` that brings paren_depth to 0.
                    let mut found_close = false;
                    let mut close_pos = 0;
                    let mut temp_depth = depth;
                    for (i, c) in text.char_indices() {
                        if c == ')' {
                            temp_depth -= 1;
                            if temp_depth == 0 {
                                close_pos = i;
                                found_close = true;
                                break;
                            }
                        }
                    }

                    if found_close {
                        // Push everything before the closing `)`
                        content.push_str(&text[..close_pos]);

                        // Text after the `)` needs to be re-lexed and injected.
                        let after = &text[close_pos + 1..];

                        // Remove the Comment token itself.
                        self.tokens.remove(idx);
                        if self.current >= idx && self.current > 0 {
                            self.current = idx;
                        } else {
                            self.current = idx;
                        }

                        // Re-lex the after-text and inject as new tokens.
                        if !after.trim().is_empty() {
                            use logos::Logos;
                            let after_start = cm_start + close_pos + 1;
                            let mut sub_lex = Token::lexer(after);
                            let mut inject: Vec<(Token, usize, usize)> = Vec::new();
                            while let Some(tok_result) = sub_lex.next() {
                                let span = sub_lex.span();
                                if let Ok(tok) = tok_result {
                                    inject.push((
                                        tok,
                                        after_start + span.start,
                                        after_start + span.end,
                                    ));
                                }
                            }
                            for (j, t) in inject.iter().enumerate() {
                                self.tokens.insert(idx + j, t.clone());
                            }
                        }

                        depth = 0; // The `)` closed our outer paren
                    } else {
                        // No `)` found — just consume the Comment as literal text.
                        content.push_str(&text);
                        self.next();
                    }
                }
                Some(Token::Escape) => {
                    // Escaped character like \( or \). Consume the escape and
                    // the next token without affecting parenthesis depth.
                    if let Some(text) = self.get_current_text() {
                        content.push_str(&text);
                    }
                    self.next();
                    // Also consume the escaped character
                    if let Some(text) = self.get_current_text() {
                        content.push_str(&text);
                    }
                    self.next();
                }
                Some(Token::Heredoc) | Some(Token::HeredocTabs) => {
                    // Heredoc inside $(...): skip the heredoc body tokens.
                    // The parentheses inside the heredoc body should NOT affect
                    // the depth tracking.
                    let heredoc_text = self.get_current_text().unwrap_or_default();
                    content.push_str(&heredoc_text);
                    self.next(); // consume << or <<-

                    // Skip whitespace before delimiter
                    while matches!(
                        self.peek(),
                        Some(Token::Space | Token::Tab | Token::Comment)
                    ) {
                        if let Some(text) = self.get_current_text() {
                            content.push_str(&text);
                        }
                        self.next();
                    }

                    // Consume delimiter and save the delimiter text.
                    let mut delim_str = String::new();
                    if matches!(self.peek(), Some(Token::Escape)) {
                        if let Some(text) = self.get_current_text() {
                            content.push_str(&text);
                        }
                        self.next(); // consume backslash
                                     // After backslash, the next token is the delimiter word
                        if let Some(text) = self.get_current_text() {
                            delim_str = text.to_string();
                            content.push_str(&text);
                        }
                        self.next(); // consume delimiter word
                    } else {
                        // Get the raw delimiter text (may be an over-greedy
                        // SingleQuotedString that swallowed past the delimiter).
                        if let Some(text) = self.get_current_text() {
                            let raw_text = text.to_string();
                            // If the token contains a newline, logos created an
                            // over-greedy SingleQuotedString that swallowed part of
                            // the heredoc body.  We need to extract just the actual
                            // delimiter and then scan the raw input for the body.
                            if raw_text.contains('\n') {
                                // Truncate at first newline to get the delimiter
                                let nl_pos = raw_text.find('\n').unwrap();
                                let truncated = raw_text[..nl_pos].to_string();
                                // Strip surrounding quotes from the truncated text.
                                // After parse_word strips external quotes from a
                                // SingleQuotedString, the truncated text may still
                                // have a trailing quote from the delimiter's closing
                                // quote that was consumed as a regular character.
                                let clean = if truncated.starts_with('\'')
                                    && truncated.ends_with('\'')
                                {
                                    truncated[1..truncated.len() - 1].to_string()
                                } else if truncated.ends_with('\'') || truncated.ends_with('"') {
                                    truncated[..truncated.len() - 1].to_string()
                                } else {
                                    truncated
                                };
                                delim_str = clean;
                                content.push_str(&delim_str);
                                // Don't add a separate newline — the over-greedy
                                // token already consumed it.  Skip past the token.
                                self.next(); // consume the over-greedy delimiter token
                                             // The current token is now past the over-greedy string.
                                             // We need to scan the raw input for the heredoc body.
                                             // Find the position right after the '<<' token in raw input.
                                let heredoc_end = {
                                    // Find the Heredoc token position
                                    let mut h_end = 0;
                                    for tok in &self.tokens {
                                        if matches!(tok.0, Token::Heredoc | Token::HeredocTabs) {
                                            h_end = tok.2;
                                            break;
                                        }
                                    }
                                    h_end
                                };
                                // The body starts after the first newline following <<
                                let body_start = match self.input[heredoc_end..].find('\n') {
                                    Some(nl_offset) => heredoc_end + nl_offset + 1,
                                    None => self.input.len(),
                                };
                                let start_pos = body_start;
                                // Scan raw input line by line until we find the delimiter
                                if !delim_str.is_empty() {
                                    let input_bytes = self.input.as_bytes();
                                    let mut current_pos = start_pos;
                                    while current_pos < self.input.len() {
                                        let line_end = self.input[current_pos..]
                                            .find('\n')
                                            .map(|i| current_pos + i)
                                            .unwrap_or(self.input.len());
                                        let line = &self.input[current_pos..line_end];
                                        if line.trim() == delim_str {
                                            content.push_str(line);
                                            content.push('\n');
                                            current_pos = line_end.saturating_add(1);
                                            break;
                                        }
                                        content.push_str(line);
                                        if line_end < self.input.len()
                                            && input_bytes[line_end] == b'\n'
                                        {
                                            content.push('\n');
                                            current_pos = line_end + 1;
                                        } else {
                                            current_pos = line_end;
                                            break;
                                        }
                                    }
                                    // Advance past all tokens up to current_pos
                                    loop {
                                        match self.tokens.get(self.current) {
                                            Some((_, start, _)) if *start < current_pos => {
                                                self.next();
                                            }
                                            _ => break,
                                        }
                                    }
                                }
                            } else {
                                // Normal case: delimiter token without newline
                                // Strip quotes from single-quoted or double-quoted delimiters
                                let clean_delim =
                                    if raw_text.starts_with('\'') && raw_text.ends_with('\'') {
                                        raw_text[1..raw_text.len() - 1].to_string()
                                    } else if raw_text.starts_with('"') && raw_text.ends_with('"') {
                                        raw_text[1..raw_text.len() - 1].to_string()
                                    } else {
                                        raw_text.clone()
                                    };
                                delim_str = clean_delim;
                                content.push_str(&delim_str);
                                self.next(); // consume delimiter word

                                // Add the newline after the delimiter word to content
                                if let Some(text) = self.get_current_text() {
                                    content.push_str(&text);
                                }
                                self.next(); // consume the newline after delimiter

                                // The current token is now at the start of the heredoc body.
                                let cur_pos = match self.get_span() {
                                    Some((s, _)) => s,
                                    None => return Err(ParserError::UnexpectedEOF),
                                };
                                let start_pos = cur_pos;

                                // Also extract delimiter from raw line for robustness.
                                if delim_str.is_empty() {
                                    let heredoc_line_end = cur_pos;
                                    let heredoc_line_start = self.input[..heredoc_line_end]
                                        .rfind('\n')
                                        .map(|p| p + 1)
                                        .unwrap_or(0);
                                    let line = &self.input[heredoc_line_start..heredoc_line_end];
                                    let trimmed = line.trim();
                                    if let Some(pos) = trimmed.rfind(|c: char| c.is_whitespace()) {
                                        let word = trimmed[pos + 1..].trim();
                                        delim_str = if word.starts_with('\\') {
                                            word[1..].to_string()
                                        } else {
                                            word.to_string()
                                        };
                                    }
                                }

                                if !delim_str.is_empty() {
                                    // Scan raw input line by line until we find the delimiter
                                    let input_bytes = self.input.as_bytes();
                                    let mut current_pos = start_pos;
                                    while current_pos < self.input.len() {
                                        let line_end = self.input[current_pos..]
                                            .find('\n')
                                            .map(|i| current_pos + i)
                                            .unwrap_or(self.input.len());
                                        let line = &self.input[current_pos..line_end];
                                        if line.trim() == delim_str {
                                            content.push_str(line);
                                            content.push('\n');
                                            current_pos = line_end.saturating_add(1);
                                            break;
                                        }
                                        content.push_str(line);
                                        if line_end < self.input.len()
                                            && input_bytes[line_end] == b'\n'
                                        {
                                            content.push('\n');
                                            current_pos = line_end + 1;
                                        } else {
                                            current_pos = line_end;
                                            break;
                                        }
                                    }

                                    // Advance past all tokens up to current_pos
                                    loop {
                                        match self.tokens.get(self.current) {
                                            Some((_, start, _)) if *start < current_pos => {
                                                self.next();
                                            }
                                            _ => break,
                                        }
                                    }
                                }
                            }
                        } else {
                            self.next(); // consume delimiter word (no text)
                        }
                    }
                }
                Some(_) => {
                    if let Some(text) = self.get_current_text() {
                        content.push_str(&text);
                    }
                    self.next();
                }
                None => return Err(ParserError::UnexpectedEOF),
            }
        }

        if crate::debug::is_debug_enabled() {
            eprintln!(
                "DEBUG capture_parenthetical_text: captured {} chars, returning at token idx {}",
                content.len(),
                self.current
            );
        }
        if crate::debug::is_debug_enabled() && content.len() > 50 {
            eprintln!(
                "DEBUG capture_parenthetical_text: first 50 chars: {:?}",
                &content[..50]
            );
        }
        Ok(content)
    }

    fn capture_double_bracket_expression(&mut self) -> Result<String, ParserError> {
        let mut content = String::new();
        let mut depth = 2; // Start with depth 2 for [[

        // Consume the first two [
        self.next(); // consume first [
        self.next(); // consume second [

        while depth > 0 {
            match self.peek() {
                Some(Token::TestBracket) => {
                    depth += 1;
                    content.push('[');
                    self.next();
                }
                Some(Token::TestBracketClose) => {
                    depth -= 1;
                    if depth > 0 {
                        content.push(']');
                    }
                    self.next();
                }
                Some(_) => {
                    if let Some(text) = self.get_current_text() {
                        content.push_str(&text);
                    }
                    self.next();
                }
                None => return Err(ParserError::UnexpectedEOF),
            }
        }

        Ok(content)
    }

    fn capture_single_bracket_expression(&mut self) -> Result<String, ParserError> {
        let mut content = String::new();
        let mut depth = 1; // Start with depth 1 for [

        // Consume the opening [
        self.next();

        while depth > 0 {
            match self.peek() {
                Some(Token::TestBracket) => {
                    depth += 1;
                    content.push('[');
                    self.next();
                }
                Some(Token::TestBracketClose) => {
                    depth -= 1;
                    if depth > 0 {
                        content.push(']');
                    }
                    self.next();
                }
                Some(_) => {
                    if let Some(text) = self.get_current_text() {
                        content.push_str(&text);
                    }
                    self.next();
                }
                None => return Err(ParserError::UnexpectedEOF),
            }
        }

        Ok(content)
    }

    fn get_identifier_text(&mut self) -> Result<String, ParserError> {
        if let Some(Token::Identifier) = self.peek() {
            if let Some(text) = self.get_current_text() {
                self.next();
                Ok(text)
            } else {
                Err(ParserError::InvalidSyntax(
                    "Failed to get identifier text".to_string(),
                ))
            }
        } else {
            Err(ParserError::InvalidSyntax(
                "Expected identifier".to_string(),
            ))
        }
    }

    fn get_number_text(&mut self) -> Result<String, ParserError> {
        if let Some(Token::Number) | Some(Token::Float) | Some(Token::PaddedNumber) = self.peek() {
            if let Some(text) = self.get_current_text() {
                self.next();
                Ok(text)
            } else {
                Err(ParserError::InvalidSyntax(
                    "Failed to get number text".to_string(),
                ))
            }
        } else {
            Err(ParserError::InvalidSyntax("Expected number".to_string()))
        }
    }

    fn get_raw_token_text(&mut self) -> Result<String, ParserError> {
        if let Some(text) = self.get_current_text() {
            self.next();
            Ok(text)
        } else {
            Err(ParserError::InvalidSyntax(
                "Failed to get token text".to_string(),
            ))
        }
    }

    fn get_string_text(&mut self) -> Result<String, ParserError> {
        if let Some((start, end)) = self.get_span() {
            let text = self.get_text(start, end);
            self.next();
            Ok(text)
        } else {
            Err(ParserError::InvalidSyntax(
                "Failed to get string text".to_string(),
            ))
        }
    }

    fn get_current_text(&mut self) -> Option<String> {
        if let Some((start, end)) = self.get_span() {
            Some(self.get_text(start, end))
        } else {
            None
        }
    }

    fn current_position(&mut self) -> usize {
        self.current
    }

    fn offset_to_line_col(&mut self, offset: usize) -> (usize, usize) {
        // Delegate to the lexer's implementation
        Lexer::offset_to_line_col(self, offset)
    }

    fn peek(&mut self) -> Option<Token> {
        self.tokens
            .get(self.current)
            .map(|(token, _, _)| token.clone())
    }

    fn peek_n(&mut self, n: usize) -> Option<Token> {
        self.tokens
            .get(self.current + n)
            .map(|(token, _, _)| token.clone())
    }

    fn next(&mut self) -> Option<Token> {
        if self.current < self.tokens.len() {
            let token = self.tokens[self.current].0.clone();
            self.current += 1;
            Some(token)
        } else {
            None
        }
    }

    fn is_eof(&mut self) -> bool {
        self.current >= self.tokens.len()
    }

    fn consume(&mut self, expected: Token) -> Result<(), ParserError> {
        if let Some(token) = self.peek() {
            if std::mem::discriminant(&token) == std::mem::discriminant(&expected) {
                self.next();
                Ok(())
            } else {
                // Get the current token's position for accurate error reporting
                if let Some((_, start, _)) = self.tokens.get(self.current) {
                    let (line, col) = self.offset_to_line_col(*start);
                    Err(ParserError::UnexpectedToken { token, line, col })
                } else {
                    // Fallback to current position if we can't get the span
                    let current_pos = self.current_position();
                    let (line, col) = self.offset_to_line_col(current_pos);
                    Err(ParserError::UnexpectedToken { token, line, col })
                }
            }
        } else {
            Err(ParserError::UnexpectedEOF)
        }
    }
}
