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
                Token::Space | Token::Tab | Token::Comment | Token::Newline | Token::CarriageReturn => {
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
                    // )) closes two levels of paren depth
                    // (ArithmeticEvalClose has priority over ParenClose via logos)
                    depth -= 2;
                    if depth > 0 {
                        content.push_str("))");
                    } else if depth == 0 {
                        content.push(')'); // one ) is inside, one closes
                    } else {
                        // depth went negative — we overshot; emit all ) needed
                        content.push_str("))");
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
