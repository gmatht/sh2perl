use logos::Logos;
use std::cmp::Ordering;
use thiserror::Error;

use crate::parser::errors::ParserError;

#[derive(Logos, Debug, PartialEq, Clone)]
pub enum Token {
    // Keywords
    #[token("if")]
    If,
    #[token("then")]
    Then,
    #[token("else")]
    Else,
    #[token("elif")]
    Elif,
    #[token("fi")]
    Fi,
    #[token("while")]
    While,
    #[token("do")]
    Do,
    #[token("done")]
    Done,
    #[token("for")]
    For,
    #[token("in")]
    In,
    #[token("function")]
    Function,
    #[token("case")]
    Case,
    #[token("esac")]
    Esac,
    #[token("select")]
    Select,
    #[token("until")]
    Until,
    #[token("break")]
    Break,
    #[token("continue")]
    Continue,
    #[token("return")]
    Return,
    #[token("exit")]
    Exit,
    #[token("export")]
    Export,
    #[token("readonly")]
    Readonly,
    Local,
    #[token("declare")]
    Declare,
    #[token("typeset")]
    Typeset,
    #[token("unset")]
    Unset,
    #[token("shift")]
    Shift,
    #[token("set")]
    Set,
    #[token("eval")]
    Eval,
    #[token("exec")]
    Exec,
    #[token("source")]
    Source,
    // SourceDot removed - dots in filenames should be part of identifiers
    #[token("trap")]
    Trap,
    #[token("wait")]
    Wait,
    #[token("shopt")]
    Shopt,
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("[")]
    TestBracket,
    #[token("]")]
    TestBracketClose,

    // Operators
    #[token("|")]
    Pipe,
    #[token("||", priority = 1)]
    Or,
    #[token("&")]
    Background,
    #[token("&&", priority = 1)]
    And,
    #[token(";")]
    Semicolon,
    #[token(",")]
    Comma,
    #[token(";;", priority = 1)]
    DoubleSemicolon,
    #[token("..", priority = 3)]
    Range,
    #[token("(")]
    ParenOpen,
    #[token(")")]
    ParenClose,
    #[token("{")]
    BraceOpen,
    #[token("}")]
    BraceClose,
    #[token("==", priority = 1)]
    Equality,
    #[token("=")]
    Assign,
    #[token("%=", priority = 3)]
    PercentAssign,
    #[token("**=", priority = 3)]
    StarStarAssign,
    #[token("<<=", priority = 3)]
    LeftShiftAssign,
    #[token(">>=", priority = 2)]
    RightShiftAssign,
    #[token("&=", priority = 3)]
    AndAssign,
    #[token("^=", priority = 3)]
    CaretAssign,
    #[token("|=", priority = 3)]
    OrAssign,

    // Redirections
    #[token("<")]
    RedirectIn,
    #[token(">>", priority = 0)]
    RedirectAppend,
    #[token(">")]
    RedirectOut,
    #[token("<>", priority = 1)]
    RedirectInOut,
    #[token("<<", priority = 1)]
    Heredoc,
    #[token("<<-", priority = 1)]
    HeredocTabs,
    #[token("<<<", priority = 1)]
    HereString,
    #[token(">&", priority = 1)]
    RedirectOutErr,
    #[token("<&", priority = 1)]
    RedirectInErr,
    #[token(">|", priority = 1)]
    RedirectOutClobber,
    #[token("&>", priority = 1)]
    RedirectAll,
    #[token("&>>", priority = 1)]
    RedirectAllAppend,

    // Variables and expansions
    #[token("$", priority = 2)]
    Dollar,
    #[token("${")]
    DollarBrace,
    #[token("$(")]
    DollarParen,
    #[token("$#", priority = 3)]
    DollarHashSimple,
    #[token("$@", priority = 3)]
    DollarAtSimple,
    #[token("$*", priority = 3)]
    DollarStarSimple,
    #[token("$?", priority = 3)]
    DollarQuestion,
    #[token("$$", priority = 3)]
    DollarDollar,
    #[token("$!", priority = 3)]
    DollarBang,
    #[token("$-", priority = 3)]
    DollarMinus,
    // Backtick token not currently used
    #[token("`", priority = 1)]
    _Backtick, // Unused variant, prefixed with underscore
    #[token("${#", priority = 3)]
    DollarBraceHash,
    #[token("${!", priority = 3)]
    DollarBraceBang,
    #[token("${*", priority = 3)]
    DollarBraceStar,
    #[token("${@", priority = 3)]
    DollarBraceAt,
    #[token("${#*", priority = 3)]
    DollarBraceHashStar,
    #[token("${#@", priority = 3)]
    DollarBraceHashAt,
    #[token("${!*", priority = 3)]
    DollarBraceBangStar,
    #[token("${!@", priority = 3)]
    DollarBraceBangAt,

    // Arithmetic
    #[token("$((", priority = 0)]
    Arithmetic,
    #[token("((", priority = 0)]
    ArithmeticEval,
    #[token("))", priority = 0)]
    ArithmeticEvalClose,
    #[token("$[")]
    ArithmeticBracket,
    #[token("let")]
    Let,

    // Conditionals
    #[token("-eq", priority = 1)]
    Eq,
    #[token("-ne", priority = 1)]
    Ne,
    #[token("-lt", priority = 1)]
    Lt,
    #[token("-le", priority = 1)]
    Le,
    #[token("-gt", priority = 1)]
    Gt,
    #[token("-ge", priority = 1)]
    Ge,
    #[token("-z", priority = 1)]
    Zero,
    #[token("-n", priority = 1)]
    NonZero,
    #[token("-f", priority = 1)]
    File,
    #[token("-d", priority = 1)]
    Directory,
    #[token("-e", priority = 1)]
    Exists,
    #[token("-r", priority = 10)]
    Readable,
    #[token("-w", priority = 1)]
    Writable,
    #[token("-x", priority = 1)]
    Executable,
    #[token("-s", priority = 1)]
    Size,
    #[token("-L", priority = 1)]
    Symlink,
    #[token("-h", priority = 1)]
    SymlinkH,
    #[token("-p", priority = 1)]
    PipeFile,
    #[token("-S", priority = 1)]
    Socket,
    #[token("-b", priority = 1)]
    Block,
    #[token("-c", priority = 1)]
    Character,
    #[token("-g", priority = 1)]
    SetGid,
    #[token("-k", priority = 1)]
    Sticky,
    #[token("-u", priority = 1)]
    SetUid,
    #[token("-O", priority = 1)]
    Owned,
    #[token("-G", priority = 1)]
    GroupOwned,
    #[token("-N", priority = 1)]
    Modified,
    #[token("-nt", priority = 1)]
    NewerThan,
    #[token("-ot", priority = 1)]
    OlderThan,
    #[token("-ef", priority = 1)]
    SameFile,

    // Command-line flags (general)
    #[token("-name")]
    NameFlag,
    #[token("-maxdepth")]
    MaxDepthFlag,
    #[token("-type")]
    TypeFlag,

    // Regex matching
    #[token("=~")]
    RegexMatch,

    // Strings and literals
    #[regex(r#""([^"\\]|\\\n|\\.)*""#, priority = 4)]
    DoubleQuotedString,
    #[regex(r"'[^']*'", priority = 3)]
    SingleQuotedString,
    #[regex(r"`([^`\\]|\\\n|\\.)*`", priority = 3)]
    BacktickString,
    #[regex(r"\$'([^'\\]|\\.)*'", priority = 3)]
    DollarSingleQuotedString,
    #[regex(r#"\$"([^"\\]|\\.)*""#, priority = 3)]
    DollarDoubleQuotedString,

    // Long options (must come before Identifier to avoid conflicts)
    // Match both --option=value and --option (without =value)
    // Note: use raw string r##"..."## to allow double quotes inside
    #[regex(r##"--[a-zA-Z][a-zA-Z0-9_*?.-]*(=("[^"]*"|'[^']*'|[^ \t\n\r|&;(){}<>"'`$\[\]\?#!@*]*))?"##, priority = 3)]
    LongOption,

    // Identifiers and words
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_*?\-]*", priority = 2)]
    Identifier,

    #[regex(r"[0-9]+")]
    Number,
    #[regex(r"[0-9]+\.[0-9]+")]
    Float,
    #[regex(r"0x[0-9a-fA-F]+")]
    HexNumber,
    #[regex(r"0+[0-9]+")]
    PaddedNumber,

    // Special characters
    #[token("!")]
    Bang,
    // #[token("#", priority = 1)]
    // _Hash, // Unused variant, prefixed with underscore
    #[token("%", priority = 2)]
    Percent,
    #[token("^", priority = 2)]
    Caret,
    #[token("~")]
    Tilde,
    #[token("+")]
    Plus,
    #[token("+=", priority = 3)]
    PlusAssign,
    #[token("-")]
    Minus,
    #[token("-=", priority = 3)]
    MinusAssign,
    #[token("*")]
    Star,
    #[token("*=", priority = 3)]
    StarAssign,
    #[token("/")]
    Slash,
    #[token("/=", priority = 3)]
    SlashAssign,
    #[token("\\", priority = 1)]
    _Backslash, // Unused variant, prefixed with underscore
    #[token("?")]
    Question,
    #[token(".")]
    Dot,
    #[regex(
        r"\*[a-zA-Z0-9_*?]*|\[[a-zA-Z0-9\-]+\]|\[[a-zA-Z0-9\-]+\]\[[a-zA-Z0-9\-]+\]",
        priority = 1
    )]
    CasePattern,
    #[token(":", priority = 1)]
    Colon,
    #[token("@")]
    At,
    #[token("`", priority = 2)]
    BacktickChar,
    #[token("'")]
    SingleQuote,
    #[token("\"")]
    DoubleQuote,
    #[token("\\", priority = 2)]
    Escape,
    // Escaped double-quote: \" — must have higher priority than Escape
    // so logos matches the full \" sequence before individual tokens.
    // This prevents DoubleQuotedString regex from seeing the " and
    // attempting a greedy match that fails inside ${...} expansions.
    #[regex(r#"\\""#, priority = 6)]
    EscapedDoubleQuote,
    // Escaped single-quote: backslash-quote - higher priority than Escape
    // so that backslash-quote is matched as a single token.
    #[regex(r"\\'", priority = 6)]
    EscapedSingleQuote,
    #[regex(r"\n", priority = 5)]
    Newline,
    #[token("\r")]
    CarriageReturn,
    #[token("\t")]
    Tab,
    #[regex(r" +", priority = 3)]
    Space,

    // Comments
    #[regex(r"#[^\r\n]*", priority = 10)]
    Comment,

    // Regex pattern content (for bash test expressions)
    #[regex(r"\^[a-zA-Z0-9\-\[\]\+\.\$\*\?\\|:#/!^_]+", priority = 1)]
    RegexPattern,
}

#[derive(Error, Debug)]
pub enum LexerError {
    #[error("Unexpected character: {ch} at {line}:{col}")]
    UnexpectedChar { ch: char, line: usize, col: usize },
    #[error("Unterminated string")]
    _UnterminatedString, // Unused variant, prefixed with underscore
    #[error("Invalid escape sequence")]
    _InvalidEscape, // Unused variant, prefixed with underscore
}

pub struct Lexer {
    pub tokens: Vec<(Token, usize, usize)>,
    pub current: usize,
    pub input: String,
    pub line_starts: Vec<usize>,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        let mut tokens = Vec::new();
        let mut lexer = Token::lexer(input);

        while let Some(token_result) = lexer.next() {
            let span = lexer.span();
            match token_result {
                Ok(token) => tokens.push((token, span.start, span.end)),
                Err(_) => {
                    // Skip invalid tokens (logos couldn't match anything)
                    continue;
                }
            }
        }

        // Workaround for logos 0.15 bug: when a regex fails after consuming bytes,
        // logos may stop producing tokens even though input remains.  Re-lex
        // the untokenized tail with a fresh logos instance, skipping any bare
        // ' characters that the SingleQuotedString regex cannot handle.
        if let Some(&(_, _, last_end)) = tokens.last() {
            if last_end < input.len() {
                let remaining = &input[last_end..];
                let mut skip = 0;
                while skip < remaining.len() && remaining.as_bytes()[skip] == b'\'' {
                    tokens.push((Token::SingleQuote, last_end + skip, last_end + skip + 1));
                    skip += 1;
                }
                if skip > 0 {
                    let remaining = &remaining[skip..];
                    if !remaining.is_empty() {
                        let mut resume = Token::lexer(remaining);
                        while let Some(token_result) = resume.next() {
                            let span = resume.span();
                            match token_result {
                                Ok(tok) => {
                                    tokens.push((tok, last_end + skip + span.start, last_end + skip + span.end));
                                }
                                Err(_) => continue,
                            }
                        }
                    }
                } else {
                    let mut resume = Token::lexer(remaining);
                    while let Some(token_result) = resume.next() {
                        let span = resume.span();
                        match token_result {
                            Ok(tok) => {
                                tokens.push((tok, last_end + span.start, last_end + span.end));
                            }
                            Err(_) => continue,
                        }
                    }
                }
            }
        }

        // Post-process: remove backslash-newline continuations.
        // A `\` immediately followed by `\n` is a line continuation;
        // skip both tokens so the parser sees them as whitespace.
        {
            let mut i = 0;
            while i < tokens.len() {
                let is_backslash = matches!(tokens[i].0, Token::_Backslash | Token::Escape);
                if is_backslash
                    && i + 1 < tokens.len()
                    && matches!(tokens[i + 1].0, Token::Newline | Token::CarriageReturn)
                {
                    tokens.remove(i);      // remove backslash
                    tokens.remove(i);      // remove newline (indices shifted)
                    // Don't increment i — the next token is now at position i
                } else {
                    i += 1;
                }
            }
        }

        // Post-process: re-parse DoubleQuotedString tokens to properly
        // handle $(...) and ${...} nesting. Logos's regex splits on every
        // " even inside $(...)/${...}, so we manually scan from each
        // opening " forward, tracking nesting, to find the real closing ".
        Self::merge_double_quoted_strings(input, &mut tokens);

        // Split over-greedy SingleQuotedString tokens that span multiple
        // lines and contain shell keywords.
        Self::split_overgreedy_sq(input, &mut tokens);

        // Precompute starts of lines

        // Precompute starts of lines for quick offset->(line,col)
        let mut line_starts = Vec::new();
        line_starts.push(0);
        let mut i = 0;
        while i < input.len() {
            if input.as_bytes()[i] == b'\r'
                && i + 1 < input.len()
                && input.as_bytes()[i + 1] == b'\n'
            {
                // Windows line ending: \r\n - only count \n as line break
                if i + 2 < input.len() {
                    line_starts.push(i + 2);
                }
                i += 2;
            } else if input.as_bytes()[i] == b'\n' {
                // Unix line ending: \n
                if i + 1 < input.len() {
                    line_starts.push(i + 1);
                }
                i += 1;
            } else if input.as_bytes()[i] == b'\r' {
                // Lone \r (old Mac line ending)
                if i + 1 < input.len() {
                    line_starts.push(i + 1);
                }
                i += 1;
            } else {
                i += 1;
            }
        }

        Self {
            tokens,
            current: 0,
            input: input.to_string(),
            line_starts,
        }
    }

    pub fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.current).map(|(token, _, _)| token)
    }

    pub fn peek_n(&self, n: usize) -> Option<&Token> {
        self.tokens.get(self.current + n).map(|(token, _, _)| token)
    }

    pub fn next(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.current).map(|(token, _, _)| token);
        self.current += 1;
        token
    }

    pub fn consume(&mut self, expected: Token) -> Result<(), LexerError> {
        if let Some(token) = self.next() {
            if std::mem::discriminant(token) == std::mem::discriminant(&expected) {
                Ok(())
            } else {
                // Get the actual character from the current token for better error reporting
                // Note: self.current was incremented by next(), so we need to look at current - 1
                if let Some((_, start, end)) = self.tokens.get(self.current - 1) {
                    let actual_char = self.input[*start..*end].chars().next().unwrap_or('?');
                    let (line, col) = self.offset_to_line_col(*start);
                    Err(LexerError::UnexpectedChar {
                        ch: actual_char,
                        line,
                        col,
                    })
                } else {
                    Err(LexerError::UnexpectedChar {
                        ch: '?',
                        line: 1,
                        col: 1,
                    })
                }
            }
        } else {
            Err(LexerError::UnexpectedChar {
                ch: '?',
                line: 1,
                col: 1,
            })
        }
    }

    pub fn is_eof(&self) -> bool {
        self.current >= self.tokens.len()
    }

    pub fn current_position(&self) -> usize {
        self.current
    }

    pub fn get_span(&self) -> Option<(usize, usize)> {
        self.tokens
            .get(self.current)
            .map(|(_, start, end)| (*start, *end))
    }

    pub fn get_text(&self, start: usize, end: usize) -> String {
        self.input[start..end].to_string()
    }

    pub fn get_current_text(&self) -> Option<String> {
        self.tokens
            .get(self.current)
            .map(|(_, start, end)| self.input[*start..*end].to_string())
    }

    pub fn get_position(&self) -> usize {
        self.current
    }

    pub fn has_newline_before_current_token(&self) -> bool {
        if self.current == 0 {
            return false;
        }

        // Look at the previous tokens to see if there was a newline
        for i in (0..self.current).rev() {
            if let Some((token, _, _)) = self.tokens.get(i) {
                match token {
                    Token::Newline | Token::CarriageReturn => return true,
                    Token::Space | Token::Tab | Token::Comment => continue, // Skip whitespace
                    _ => return false, // Found a non-whitespace token before newline
                }
            }
        }
        false
    }
}

impl Lexer {
    /// Scan forward from the current token (which must be a DoubleQuote at
    /// a `"` byte) through the raw input bytes to find the matching closing
    /// `"`, handling backslash-newline continuations and `$(...)`/`${...}`
    /// nesting. Returns the captured substring (including both quotes) WITH
    /// backslash-newline continuations removed so that the result is a clean
    /// single-line string suitable for re-tokenization as a DoubleQuotedString.
    /// Advances the lexer past all tokens that fall within the captured span.
    pub fn scan_double_quoted_string(&mut self) -> Result<String, ParserError> {
        use crate::parser::errors::ParserError;

        let start = self.tokens[self.current].1;
        let bytes = self.input.as_bytes();
        if bytes[start] != b'"' {
            return Err(ParserError::InvalidSyntax(
                "scan_double_quoted_string called on non-quote token".to_string(),
            ));
        }

        let mut result = String::new();
        result.push('"'); // opening quote

        let mut pos = start + 1;
        let mut p_depth = 0i32;
        let mut b_depth = 0i32;

        while pos < bytes.len() {
            match bytes[pos] {
                b'"' if p_depth == 0 && b_depth == 0 => {
                    result.push('"'); // closing quote
                    pos += 1;
                    break;
                }
                b'\\' if pos + 1 < bytes.len() && bytes[pos + 1] == b'\n' => {
                    // Backslash-newline continuation: skip both bytes (do not copy)
                    pos += 2;
                }
                b'\\' if pos + 1 < bytes.len() => {
                    // Other escaped char: copy backslash and the escaped char
                    result.push('\\');
                    pos += 1;
                    result.push(bytes[pos] as char);
                    pos += 1;
                }
                b'$' if pos + 1 < bytes.len() && bytes[pos + 1] == b'(' => {
                    p_depth += 1;
                    result.push('$');
                    result.push('(');
                    pos += 2;
                }
                b'$' if pos + 1 < bytes.len() && bytes[pos + 1] == b'{' => {
                    b_depth += 1;
                    result.push('$');
                    result.push('{');
                    pos += 2;
                }
                b')' => {
                    if p_depth > 0 {
                        p_depth -= 1;
                    }
                    result.push(')');
                    pos += 1;
                }
                b'}' => {
                    if b_depth > 0 {
                        b_depth -= 1;
                    }
                    result.push('}');
                    pos += 1;
                }
                _ => {
                    result.push(bytes[pos] as char);
                    pos += 1;
                }
            }
        }

        // Advance lexer past all tokens that are within the captured span
        let end_pos = pos;
        while self.current < self.tokens.len() && self.tokens[self.current].1 < end_pos {
            self.current += 1;
        }

        Ok(result)
    }

    /// Scan from the current token (which must be a Comment token at a `#` byte)
    /// through the raw input to find the closing `))` of an arithmetic expression,
    /// handling `#` as the base-notation operator (e.g. `10#$x`).  Returns the
    /// captured substring (including the `#` but NOT the closing `))`).
    /// Re-injects any text after `))` (e.g. `; then`) as new tokens so the
    /// caller can continue parsing normally.
    pub fn scan_arithmetic_comment(&mut self) -> String {
        let start = self.tokens[self.current].1;
        let bytes = self.input.as_bytes();
        let mut i = start;
        // Skip the `#`
        if i < bytes.len() && bytes[i] == b'#' {
            i += 1;
        }
        // Scan forward until we find the closing `))` or end of line
        let mut paren_depth = 0i32;
        let mut closing_pos = None;
        while i < bytes.len() && bytes[i] != b'\n' && bytes[i] != b'\r' {
            if bytes[i] == b')' {
                if i + 1 < bytes.len() && bytes[i + 1] == b')' {
                    // Found closing `))`
                    closing_pos = Some(i);
                    break;
                }
                paren_depth -= 1;
                if paren_depth < 0 {
                    break;
                }
            } else if bytes[i] == b'(' {
                paren_depth += 1;
            }
            i += 1;
        }
        
        let captured = self.input[start..i].to_string();

        // Build list of tokens to inject:
        //   1. ArithmeticEvalClose at the "))" position
        //   2. Re-lexed tokens for text after "))" up to the newline
        //      (the original Comment swallowed this text; we must recreate it).
        let mut inject_tokens: Vec<(Token, usize, usize)> = Vec::new();
        if let Some(cp) = closing_pos {
            inject_tokens.push((Token::ArithmeticEvalClose, cp, cp + 2));

            let after_parens = cp + 2;
            let remaining = &self.input[after_parens..];
            // Only re-lex up to the newline (the Comment covered everything to EOL).
            let line_end = remaining.find('\n').unwrap_or(remaining.len());
            let before_nl = &remaining[..line_end];
            if !before_nl.is_empty() && !before_nl.trim().is_empty() {
                let mut sub_lexer = Token::lexer(before_nl);
                while let Some(token_result) = sub_lexer.next() {
                    let span = sub_lexer.span();
                    match token_result {
                        Ok(tok) => {
                            inject_tokens.push((
                                tok,
                                after_parens + span.start,
                                after_parens + span.end,
                            ));
                        }
                        Err(_) => continue,
                    }
                }
            }
        }

        // ---- Remove stale tokens ----
        // The Comment token at self.current spans from `#` to the newline.
        // Any tokens between the Comment and the first token after the newline
        // are stale (they were subsumed by the Comment).  Remove them all.
        let after_comment_end = self.tokens[self.current].2; // Comment's byte end
        let remove_start_idx = self.current;     // Remove the Comment itself
        let mut remove_end_idx = remove_start_idx + 1;
        while remove_end_idx < self.tokens.len() {
            if self.tokens[remove_end_idx].1 >= after_comment_end {
                break; // First token that starts at or after Comment's end (usually Newline)
            }
            remove_end_idx += 1;
        }
        let removed_len = remove_end_idx - remove_start_idx;

        if removed_len > 0 {
            self.tokens.drain(remove_start_idx..remove_end_idx);
            if self.current >= remove_end_idx {
                self.current = self.current.saturating_sub(removed_len);
            } else if self.current >= remove_start_idx {
                self.current = remove_start_idx;
            }
        }

        // Insert injected tokens at the position where the Comment was.
        let insert_at = remove_start_idx;
        for (j, st) in inject_tokens.iter().enumerate() {
            self.tokens.insert(insert_at + j, st.clone());
        }

        // Point current at the first injected token (ArithmeticEvalClose).
        self.current = remove_start_idx;
        captured
    }

    /// Handle a Comment token that appears inside `${...}` where `#` is a
    /// parameter-expansion operator, not a comment start.  The Comment may
    /// have consumed the closing `}` and subsequent text (e.g. `#* } ]; then`).
    /// This method:
    ///   1. Finds the first `}` in the comment text.
    ///   2. Returns everything from `#` up to (but not including) that `}`.
    ///   3. Re-injects any text after `}` as newly-lexed tokens so the
    ///      caller can continue parsing normally.
    pub fn handle_comment_with_brace(&mut self, mut brace_depth: usize) -> Result<String, ParserError> {
        let idx = self.current;
        let start = self.tokens[idx].1;
        let end = self.tokens[idx].2;
        let text = self.input[start..end].to_string();

        if let Some(pos) = text.find('}') {
            let before = &text[..pos];       // content up to `}`
            let after  = &text[pos + 1..];   // content after `}`

            // Remove the Comment token itself; we are going to replace it.
            self.tokens.remove(idx);
            if self.current >= idx && self.current > 0 {
                self.current -= 1;
            }

            // Build tokens to inject: none (the `}` is implicit because we
            // break brace_depth to 0).  But re-lex the `after` text and
            // inject those tokens.
            let mut inject: Vec<(Token, usize, usize)> = Vec::new();
            if !after.trim().is_empty() {
                // Map positions relative to the original comment start
                let comment_start = start;
                let after_start = comment_start + pos + 1;
                let mut sub = Token::lexer(after);
                while let Some(tok) = sub.next() {
                    let span = sub.span();
                    if let Ok(t) = tok {
                        inject.push((t, after_start + span.start, after_start + span.end));
                    }
                }
            }

            // Insert injected tokens at the Comment's old position.
            let insert_at = idx;
            for (j, t) in inject.iter().enumerate() {
                self.tokens.insert(insert_at + j, t.clone());
            }

            // Point current at the first injected token (which sits at idx),
            // or at idx (the position where the Comment was removed) if
            // nothing was injected.  The token at idx after removal (or the
            // first injected token) is the next token after the `}` that the
            // caller should process.  We must NOT leave current pointing at
            // an already-consumed token.
            if !inject.is_empty() {
                // Tokens were injected — point at the first one (at idx).
                self.current = idx;
            } else if self.current >= idx && self.current > 0 {
                // Nothing injected — skip past the Comment position.
                self.current = idx;
            } else {
                // self.current < idx — advance past the Comment.
                self.current = idx;
            }

            Ok(before.to_string())
        } else {
            // No `}` found — consume the Comment as literal text.
            self.current += 1;
            Ok(text)
        }
    }

    pub fn offset_to_line_col(&self, offset: usize) -> (usize, usize) {
        if self.line_starts.is_empty() {
            return (1, offset + 1);
        }
        // Binary search for the greatest line_start <= offset
        let mut left = 0usize;
        let mut right = self.line_starts.len();
        while left < right {
            let mid = (left + right) / 2;
            match self.line_starts[mid].cmp(&offset) {
                Ordering::Greater => right = mid,
                _ => left = mid + 1,
            }
        }
        let idx = left.saturating_sub(1);
        let line_start = self.line_starts.get(idx).cloned().unwrap_or(0);
        let line = idx + 1;
        let col = offset.saturating_sub(line_start) + 1;
        (line, col)
    }
    /// Re-parse DoubleQuotedString tokens to properly handle nesting
    /// of $(...), ${...}, and backtick command substitutions.
    /// Logos's regex splits on every " even inside nested constructs,
    /// so we manually scan from each opening " forward, tracking
    /// nesting depth, to find the real closing ".
    pub fn merge_double_quoted_strings(input: &str, tokens: &mut Vec<(Token, usize, usize)>) {
        let mut merged: Vec<(Token, usize, usize)> = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            if tokens[i].0 == Token::DoubleQuotedString {
                let start = tokens[i].1;
                let bytes = input.as_bytes();
                // Only re-parse if this " is at byte position with "
                if bytes[start] == b'"' {
                    let mut end = start + 1; // skip past opening "
                    let mut p_depth = 0i32;
                    let mut b_depth = 0i32;
                    let mut bt_depth = 0i32; // backtick depth
                    while end < bytes.len() {
                        match bytes[end] {
                            b'"' if p_depth == 0 && b_depth == 0 && bt_depth == 0 => {
                                end += 1; // include closing "
                                break;
                            }
                            b'\\' if end + 1 < bytes.len() => {
                                end += 2; // skip escaped char
                            }
                            b'`' => {
                                // Toggle backtick depth — backticks inside double
                                // quotes are command substitutions and should not
                                // cause the inner " to close the outer string.
                                bt_depth = if bt_depth == 0 { 1 } else { 0 };
                                end += 1;
                            }
                            b'$' if end + 1 < bytes.len() && bytes[end + 1] == b'(' => {
                                p_depth += 1;
                                end += 2;
                            }
                            b'$' if end + 1 < bytes.len() && bytes[end + 1] == b'{' => {
                                b_depth += 1;
                                end += 2;
                            }
                            b')' => {
                                if p_depth > 0 {
                                    p_depth -= 1;
                                }
                                end += 1;
                            }
                            b'}' => {
                                if b_depth > 0 {
                                    b_depth -= 1;
                                }
                                end += 1;
                            }
                            _ => {
                                end += 1;
                            }
                        }
                    }
                    merged.push((Token::DoubleQuotedString, start, end));
                    // Skip all logos tokens covered by this span
                    while i + 1 < tokens.len() && tokens[i + 1].1 < end {
                        i += 1;
                    }
                    i += 1;
                    continue;
                }
            }
            merged.push(tokens[i].clone());
            i += 1;
        }
        *tokens = merged;
    }
    /// Split over-greedy SingleQuotedString tokens that span multiple
    /// lines and contain known shell keywords after newlines.
    /// Logos's `'[^']*'` can match a closing `'` that is far away (e.g.
    /// inside a case pattern), consuming intervening shell code.  We
    /// detect such tokens and split them at the first newline that is
    /// followed by a shell keyword, turning the opening `'` into a bare
    /// `SingleQuote` token and re-tokenizing the tail with a fresh logos
    /// instance.
    pub fn split_overgreedy_sq(input: &str, tokens: &mut Vec<(Token, usize, usize)>) {
        let bytes = input.as_bytes();
        let mut result: Vec<(Token, usize, usize)> = Vec::new();

        for token in tokens.drain(..) {
            let (tok, start, end) = token;
            if tok != Token::SingleQuotedString {
                result.push((tok, start, end));
                continue;
            }

            // Only consider tokens that span at least one newline
            let span = &input[start..end];
            if !span.contains('\n') {
                result.push((tok, start, end));
                continue;
            }

            // Check if this SQ is preceded by an Escape or EscapedSingleQuote token.
            // If so, it's not over-greedy.
            let mut preceded_by_escape = false;
            if let Some(&(ref prev_tok, ref prev_end, _)) = result.last() {
                if (*prev_tok == Token::Escape || *prev_tok == Token::EscapedSingleQuote)
                    && *prev_end == start
                {
                    preceded_by_escape = true;
                }
            }
            if preceded_by_escape {
                result.push((tok, start, end));
                continue;
            }

            // Scan the content for newline followed by a shell keyword
            let content = &span[1..]; // skip opening '

            // Only split on closing/continuation keywords that indicate
            // the single-quoted string has likely overrun its bounds.
            // Opening keywords like '{', 'while', 'for', 'if', 'case',
            // 'until', 'select', 'function' can legitimately appear inside
            // multi-line quoted strings passed to awk, sed, perl, etc.
            let keywords = [
                "done", "then", "fi", "esac", "elif ",
                "do ",
            ];
            let mut split_pos = None;

            for (i, ch) in content.char_indices() {
                if ch == '\n' {
                    let mut j = i + 1;
                    while j < content.len()
                        && (content.as_bytes()[j] == b' '
                            || content.as_bytes()[j] == b'\t')
                    {
                        j += 1;
                    }
                    if j < content.len() {
                        let rest = &content[j..];
                        for kw in &keywords {
                            if rest.starts_with(kw) {
                                // Only split if the keyword is standalone on its line:
                                // after the keyword, only whitespace until newline or end.
                                let after_kw = &rest[kw.len()..];
                                let is_standalone = after_kw.is_empty()
                                    || after_kw.starts_with('\n')
                                    || after_kw.starts_with('\r')
                                    || after_kw.trim().is_empty()
                                    || after_kw.trim_start().starts_with('#');
                                if is_standalone {
                                    split_pos = Some(i);
                                    break;
                                }
                            }
                        }
                    }
                    if split_pos.is_some() {
                        break;
                    }
                }
            }

            if let Some(split_at) = split_pos {
                let body_start = start + 1;
                let split_byte = body_start + split_at;

                // Emit the opening ' as a bare SingleQuote
                result.push((Token::SingleQuote, start, start + 1));

                // Content between opening ' and split point
                if split_byte > start + 1 {
                    result.push((Token::SingleQuotedString, start + 1, split_byte));
                }

                // Re-tokenize the tail using logos
                if split_byte < end {
                    let tail = &input[split_byte..end];
                    let mut tail_lex = Token::lexer(tail);
                    while let Some(token_result) = tail_lex.next() {
                        let tail_span = tail_lex.span();
                        match token_result {
                            Ok(tok) => {
                                result.push((
                                    tok,
                                    split_byte + tail_span.start,
                                    split_byte + tail_span.end,
                                ));
                            }
                            Err(_) => continue,
                        }
                    }
                }
            } else {
                result.push((tok, start, end));
            }
        }

        *tokens = result;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokens() {
        let input = "echo hello world";
        let mut lexer = Lexer::new(input);

        assert_eq!(lexer.next(), Some(&Token::Identifier));
        assert_eq!(lexer.next(), Some(&Token::Space));
        assert_eq!(lexer.next(), Some(&Token::Identifier));
        assert_eq!(lexer.next(), Some(&Token::Space));
        assert_eq!(lexer.next(), Some(&Token::Identifier));
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_pipeline() {
        let input = "ls | grep test";
        let mut lexer = Lexer::new(input);

        assert_eq!(lexer.next(), Some(&Token::Identifier));
        assert_eq!(lexer.next(), Some(&Token::Space));
        assert_eq!(lexer.next(), Some(&Token::Pipe));
        assert_eq!(lexer.next(), Some(&Token::Space));
        assert_eq!(lexer.next(), Some(&Token::Identifier));
        assert_eq!(lexer.next(), Some(&Token::Space));
        assert_eq!(lexer.next(), Some(&Token::Identifier));
    }

    #[test]
    fn test_variables() {
        let input = "$HOME ${PATH}";
        let mut lexer = Lexer::new(input);

        assert_eq!(lexer.next(), Some(&Token::Dollar));
        assert_eq!(lexer.next(), Some(&Token::Identifier));
        assert_eq!(lexer.next(), Some(&Token::Space));
        assert_eq!(lexer.next(), Some(&Token::DollarBrace));
        assert_eq!(lexer.next(), Some(&Token::Identifier));
        assert_eq!(lexer.next(), Some(&Token::BraceClose));
    }
}
