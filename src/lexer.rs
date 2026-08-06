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
    // Conditionals — test operators for `[ -f x ]` / `[[ $x -eq 5 ]]`.
    //
    // WARNING (breakage history): these tokens lex ANYWHERE, so `-rf` in a
    // normal command was split into `-r` + `f` (longest-match takes the `-r`
    // token, `f` becomes a bare identifier). That made `rm -rf x` parse
    // identically to `rm -r f x` and forced generator workarounds that
    // conflated the two (rm.rs treated a bare `f` after `-r` as the force
    // flag, silently eating a real file named `f`). parse_word() now re-joins
    // these tokens with adjacent bare words (see parser/words.rs), so the
    // whitespace in the source is the discriminator: `-rf` (no space)
    // combines, `-r f` (space) stays two args — matching bash. Test-
    // expression parsers consume these tokens directly and are unaffected.
    // Keep them out of any non-test token consumer so the combine rule holds.
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
    #[regex(r#""([^"\\]|\\[^\n]|\\\n)*""#, priority = 4)]
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
    // NOTE: CasePattern was removed because its regex over-matched array
    // subscripts like `[ext4]` in `${mkopts[ext4]}`.  Instead, the parser
    // handles case patterns and glob patterns using individual tokens
    // (Star, TestBracket, Identifier, etc.).
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
    // Escaped backtick: backslash-backtick - higher priority than Escape
    // so that backslash-backtick is matched as a single token instead of
    // Escape followed by the start of a BacktickString regex.
    #[regex(r"\\`", priority = 6)]
    EscapedBacktick,
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
                    // Logos couldn't match anything at this position.
                    // Check if it's a bare quote character that we should keep.
                    let pos = span.start;
                    if pos < input.len() {
                        let ch = input.as_bytes()[pos];
                        if ch == b'\'' {
                            tokens.push((Token::SingleQuote, pos, pos + 1));
                            continue;
                        } else if ch == b'"' {
                            tokens.push((Token::DoubleQuote, pos, pos + 1));
                            continue;
                        } else if ch == b'`' {
                            tokens.push((Token::BacktickChar, pos, pos + 1));
                            continue;
                        }
                    }
                    // Skip other invalid tokens
                    continue;
                }
            }
        }

        // Workaround for logos 0.15 bug: when a regex fails after consuming bytes,
        // logos may stop producing tokens even though input remains.  We loop,
        // re-lexing any untokenized tail with a fresh logos instance, and skip
        // bare ' and " characters that logos cannot handle (e.g. unterminated
        // single- or double-quoted strings in the remaining input).
        loop {
            let last_end = tokens.last().map(|&(_, _, e)| e).unwrap_or(0);
            if last_end >= input.len() {
                break;
            }
            let remaining = &input[last_end..];
            // Skip bare ' and " and ` that logos may choke on
            let mut skip = 0;
            while skip < remaining.len()
                && (remaining.as_bytes()[skip] == b'\'' || remaining.as_bytes()[skip] == b'"' || remaining.as_bytes()[skip] == b'`')
            {
                let ch = remaining.as_bytes()[skip];
                if ch == b'\'' {
                    tokens.push((Token::SingleQuote, last_end + skip, last_end + skip + 1));
                } else if ch == b'"' {
                    tokens.push((Token::DoubleQuote, last_end + skip, last_end + skip + 1));
                } else {
                    tokens.push((Token::BacktickChar, last_end + skip, last_end + skip + 1));
                }
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
                                tokens.push((
                                    tok,
                                    last_end + skip + span.start,
                                    last_end + skip + span.end,
                                ));
                            }
                            Err(_) => continue,
                        }
                    }
                }
            } else {
                // No bare quotes to skip — try logos on the remaining text
                let mut resume = Token::lexer(remaining);
                while let Some(token_result) = resume.next() {
                    let span = resume.span();
                    match token_result {
                        Ok(tok) => {
                            tokens.push((
                                tok,
                                last_end + span.start,
                                last_end + span.end,
                            ));
                        }
                        Err(_) => continue,
                    }
                }
                // If logos still failed to make progress, break to avoid
                // an infinite loop (the remaining characters will be ignored).
                let new_last_end = tokens.last().map(|&(_, _, e)| e).unwrap_or(last_end);
                if new_last_end == last_end {
                    break;
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
        // Re-parse DoubleQuotedString tokens to properly handle $(...)
        // and ${...} nesting. Logos's regex splits on every
        // " even inside $(...)/${...}, so we manually scan from each
        // opening " forward, tracking nesting, to find the real closing ".
        // Run multiple passes because a merged DQS may create new bare
        // DoubleQuote tokens in the re-lexed tail that also need merging.
        Self::merge_double_quoted_strings(input, &mut tokens);
        Self::merge_double_quoted_strings(input, &mut tokens);

        // Split over-greedy SingleQuotedString tokens that span multiple
        // lines and contain shell keywords.
        Self::split_overgreedy_sq(input, &mut tokens);

        // Fix split comments: logos 0.15's `#[^\r\n]*` can fail to match
        // through `'` characters, breaking a Comment into Comment + bare tokens.
        // Re-merge any Comment with adjacent non-newline tokens on the same line.
        Self::fix_split_comments(input, &mut tokens);

        // Fix bare quotes that logos failed to pair (e.g. in fragments).
        Self::fix_bare_quotes(input, &mut tokens);

        // After fix_bare_quotes may have created new SQS tokens that overlap
        // with existing ones, run split_overgreedy_sq again to fix them.
        Self::split_overgreedy_sq(input, &mut tokens);

        // Resolve (( ambiguity: if (( cannot be closed by )), it is two
        // nested subshells rather than an arithmetic evaluation.
        Self::resolve_double_paren_ambiguity(input, &mut tokens);

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
                    // If we can't get the token, it means self.current was 0 (shouldn't happen
                    // after next() advanced it), but handle gracefully.
                    Err(LexerError::UnexpectedChar {
                        ch: '?',
                        line: 1,
                        col: 1,
                    })
                }
            }
        } else {
            // No more tokens — this is an "unexpected end of input" situation.
            // Use the last known position for a better error.
            if let Some((_, _, last_end)) = self.tokens.last() {
                let (line, col) = self.offset_to_line_col(*last_end);
                Err(LexerError::UnexpectedChar {
                    ch: '?',
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
    pub fn handle_comment_with_brace(&mut self, brace_depth: usize) -> Result<String, ParserError> {
        let idx = self.current;
        let start = self.tokens[idx].1;
        let end = self.tokens[idx].2;
        let text = self.input[start..end].to_string();

        // Scan the comment text tracking nested ${...} depth so we find the
        // correct matching `}` rather than the first one (which may belong to
        // a nested expansion like ${PATH#"${GIT_EXEC_PATH}:"}).
        let mut depth = 0;
        let mut found_pos = None;
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'$' if i + 1 < bytes.len() && bytes[i + 1] == b'{' => {
                    depth += 1;
                    i += 2;
                }
                b'}' => {
                    if depth == 0 {
                        found_pos = Some(i);
                        break;
                    }
                    depth -= 1;
                    i += 1;
                }
                b'\\' if i + 1 < bytes.len() && bytes[i + 1] == b'\\' => {
                    // Skip escaped backslash (could be part of pattern like [/\\])
                    i += 2;
                }
                _ => {
                    i += 1;
                }
            }
        }

        if let Some(pos) = found_pos {
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
            if tokens[i].0 == Token::DoubleQuotedString || tokens[i].0 == Token::DoubleQuote {
                let start = tokens[i].1;
                let bytes = input.as_bytes();
                // Only re-parse if this " is at byte position with "
                if bytes[start] == b'"' {

                    let mut end = start + 1; // skip past opening "
                    let mut p_depth = 0i32;          // $(  ) depth
                    let mut b_depth = 0i32;          // ${  } depth
                    let mut bt_depth = 0i32;          // backtick depth
                    // When inside $(), track standalone '(' that are not part of
                    // '$(' so we correctly match ')' to its corresponding '$('.
                    let mut paren_depth = 0i32;
                    // Track single-quote depth inside $(): a ' inside $() starts
                    // a single-quoted string where all characters (including ),
                    // (, \, and $) are literal and must not affect depth tracking.
                    let mut sq_depth = 0i32;
                    // Track double-quote depth inside $() / backtick contexts:
                    // a " inside $() or ` ` is a double-quoted string where ' is
                    // literal and must NOT toggle sq_depth.
                    let mut dq_depth = 0i32;
                    let mut found_close = false;
                    while end < bytes.len() {
                        let ch = bytes[end];
                        // Allow bare newlines inside double-quoted strings (valid in bash).
                        // Continue scanning until we find the matching closing quote
                        // (tracking $(...)/${...}/backtick nesting) or run out of input.
                        match ch {
                            b'"' if p_depth == 0 && b_depth == 0 && bt_depth == 0 => {
                                // Closing the outermost double-quoted string.
                                end += 1; // include closing "
                                found_close = true;
                                break;
                            }
                            b'"' if (p_depth > 0 || bt_depth > 0) && sq_depth == 0 => {
                                // Toggle double-quote depth inside $() or backtick. A
                                // `"` inside a single-quoted string within $() is a
                                // LITERAL character (bash: `'s/"//g'` inside `$(...)`
                                // inside a DQS) — it must not toggle dq_depth, or the
                                // single-quote arm below would stop seeing the closing
                                // `'` and the whole DQS would fail to find its closing
                                // `"` (parse-gaps: multiple-awk-in-dqs.sh +
                                // subshell-sed-squote-dquote.sh).
                                dq_depth = if dq_depth == 0 { 1 } else { 0 };
                                end += 1;
                            }
                            b'\\' if end + 1 < bytes.len() && sq_depth == 0 => {
                                // Backslash followed by newline is a line continuation
                                // inside double-quoted strings.  Skip both the backslash
                                // and the newline so the string spans multiple lines.
                                // Inside a single-quoted string within $(), backslash
                                // is literal and does not skip the next character.
                                if bytes[end + 1] == b'\n' {
                                    end += 2; // skip backslash AND newline (line continuation)
                                } else {
                                    end += 2; // skip escaped char
                                }
                            }
                            b'`' if sq_depth == 0 => {
                                // Toggle backtick depth — backticks inside double
                                // quotes are command substitutions and should not
                                // cause the inner " to close the outer string. Inside
                                // a single-quoted string within $() a backtick is
                                // literal (same rule as the `"` arm above).
                                bt_depth = if bt_depth == 0 { 1 } else { 0 };
                                end += 1;
                            }
                            b'$' if end + 1 < bytes.len() && bytes[end + 1] == b'(' && sq_depth == 0 => {
                                p_depth += 1;
                                end += 2;
                            }
                            b'$' if end + 1 < bytes.len() && bytes[end + 1] == b'{' && sq_depth == 0 => {
                                b_depth += 1;
                                end += 2;
                            }
                            b'\'' if p_depth > 0 && dq_depth == 0 => {
                                // Toggle single-quote depth inside $(), but only
                                // when NOT inside a double-quoted string within $().
                                // A ' inside a double-quoted string is a literal
                                // character (e.g. "\'foo'") and must not affect nesting.
                                sq_depth = if sq_depth == 0 { 1 } else { 0 };
                                end += 1;
                            }
                            b'(' if p_depth > 0 && sq_depth == 0 => {
                                // Standalone '(' inside $() — not part of '$('.
                                paren_depth += 1;
                                end += 1;
                            }
                            b')' if sq_depth == 0 => {
                                if p_depth > 0 {
                                    if paren_depth > 0 {
                                        // This ')' matches a previous '(' inside $().
                                        paren_depth -= 1;
                                    } else {
                                        // This ')' matches a '$('.
                                        p_depth -= 1;
                                    }
                                }
                                end += 1;
                            }
                            b'}' if sq_depth == 0 => {
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
                    if !found_close {
                        // No closing " found — do not merge; leave the original token(s) as-is.
                        merged.push(tokens[i].clone());
                        i += 1;
                        continue;
                    }
                    merged.push((Token::DoubleQuotedString, start, end));
                    // Skip all logos tokens covered by this span.
                    // If a token extends beyond the end, the tail portion
                    // contains real code that must be re-tokenized.
                    while i + 1 < tokens.len() {
                        let next_start = tokens[i + 1].1;
                        let next_end = tokens[i + 1].2;
                        if next_start >= end {
                            break;
                        }
                        if next_end > end {
                            // Token overlaps boundary — re-lex the tail
                            // using the same workaround as split_overgreedy_sq.
                            let tail_text = &input[end..next_end];
                            let tail_start = end;
                            let mut tail_offset = 0;
                            while tail_offset < tail_text.len() {
                                let remaining = &tail_text[tail_offset..];
                                let mut sub = Token::lexer(remaining);
                                let mut had_ok = false;
                                while let Some(token_result) = sub.next() {
                                    let span = sub.span();
                                    match token_result {
                                        Ok(tok) => {
                                            merged.push((
                                                tok,
                                                tail_start + tail_offset + span.start,
                                                tail_start + tail_offset + span.end,
                                            ));
                                            had_ok = true;
                                        }
                                        Err(_) => continue,
                                    }
                                }
                                if had_ok {
                                    if let Some(&(_, _, last_end)) = merged.last() {
                                        tail_offset = last_end - tail_start;
                                    } else {
                                        tail_offset = tail_text.len();
                                    }
                                } else {
                                    // Emit problematic byte
                                    let ch = tail_text.as_bytes()[tail_offset];
                                    if ch == b'\'' {
                                        merged.push((
                                            Token::SingleQuote,
                                            tail_start + tail_offset,
                                            tail_start + tail_offset + 1,
                                        ));
                                    } else if ch == b'"' {
                                        merged.push((
                                            Token::DoubleQuote,
                                            tail_start + tail_offset,
                                            tail_start + tail_offset + 1,
                                        ));
                                    }
                                    tail_offset += 1;
                                }
                            }
                        }
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

            // Check if this SQS starts inside a previous SQS token's span.
            // Logos can produce overlapping SingleQuotedString tokens when a
            // closing `'` of one SQS is mistakenly treated as the opening `'`
            // of a new SQS.  In that case, emit a bare SingleQuote for the
            // overlapping character and re-lex the tail (the rest of this token).
            if let Some(&(Token::SingleQuotedString, prev_start, prev_end)) = result.last() {
                if start > prev_start && start < prev_end {
                    // Opening ' is actually the closing quote of the previous SQS.
                    result.push((Token::SingleQuote, start, start + 1));
                    // Re-lex the content after this bare quote.
                    if start + 1 < end {
                        let tail_text = &input[start+1..end];
                        let tail_start = start + 1;
                        let mut tail_offset = 0;
                        while tail_offset < tail_text.len() {
                            let remaining = &tail_text[tail_offset..];
                            let mut sub = Token::lexer(remaining);
                            let mut had_ok = false;
                            while let Some(token_result) = sub.next() {
                                let span = sub.span();
                                match token_result {
                                    Ok(t) => {
                                        result.push((
                                            t,
                                            tail_start + tail_offset + span.start,
                                            tail_start + tail_offset + span.end,
                                        ));
                                        had_ok = true;
                                    }
                                    Err(_) => continue,
                                }
                            }
                            if had_ok {
                                if let Some(&(_, _, last_end)) = result.last() {
                                    tail_offset = last_end - tail_start;
                                } else {
                                    tail_offset = tail_text.len();
                                }
                            } else {
                                // Emit problematic byte as bare quote
                                let ch = tail_text.as_bytes()[tail_offset];
                                if ch == b'\'' {
                                    result.push((
                                        Token::SingleQuote,
                                        tail_start + tail_offset,
                                        tail_start + tail_offset + 1,
                                    ));
                                } else if ch == b'"' {
                                    result.push((
                                        Token::DoubleQuote,
                                        tail_start + tail_offset,
                                        tail_start + tail_offset + 1,
                                    ));
                                }
                                tail_offset += 1;
                            }
                        }
                    }
                    continue;
                }
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
                "done", "then", "fi", "esac", "elif",
                "do", ")",
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

                // The content between the opening ' and the split point is
                // NOT actually single-quoted text — it is real shell code
                // that was gobbled up by the over-greedy SQS.  Re-lex it
                // so that the shell code is properly tokenized.
                if split_byte > start + 1 {
                    let middle = &input[start + 1..split_byte];
                    let mid_start = start + 1;
                    let mut mid_offset = 0;
                    while mid_offset < middle.len() {
                        let remaining = &middle[mid_offset..];
                        let mut sub = Token::lexer(remaining);
                        let mut had_ok = false;
                        while let Some(token_result) = sub.next() {
                            let span = sub.span();
                            match token_result {
                                Ok(tok) => {
                                    result.push((
                                        tok,
                                        mid_start + mid_offset + span.start,
                                        mid_start + mid_offset + span.end,
                                    ));
                                    had_ok = true;
                                }
                                Err(_) => continue,
                            }
                        }
                        if had_ok {
                            if let Some(&(_, _, last_end)) = result.last() {
                                mid_offset = last_end - mid_start;
                            } else {
                                mid_offset = middle.len();
                            }
                        } else {
                            // Emit problematic byte
                            let ch = middle.as_bytes()[mid_offset];
                            if ch == b'\'' {
                                result.push((
                                    Token::SingleQuote,
                                    mid_start + mid_offset,
                                    mid_start + mid_offset + 1,
                                ));
                            } else if ch == b'"' {
                                result.push((
                                    Token::DoubleQuote,
                                    mid_start + mid_offset,
                                    mid_start + mid_offset + 1,
                                ));
                            }
                            mid_offset += 1;
                        }
                    }
                }

                // Re-tokenize the tail using logos
                // Logos 0.15 may stop early on unterminated strings (e.g. a '"'
                // without a matching closing '"').  We work around this by
                // looping until the entire tail is consumed, skipping bytes
                // that logos cannot tokenize.
                if split_byte < end {
                    let tail = &input[split_byte..end];
                    let tail_start = split_byte;
                    let mut tail_offset = 0;
                    while tail_offset < tail.len() {
                        let remaining = &tail[tail_offset..];
                        let mut sub = Token::lexer(remaining);
                        let mut had_ok = false;
                        while let Some(token_result) = sub.next() {
                            let span = sub.span();
                            match token_result {
                                Ok(tok) => {
                                    result.push((
                                        tok,
                                        tail_start + tail_offset + span.start,
                                        tail_start + tail_offset + span.end,
                                    ));
                                    had_ok = true;
                                }
                                Err(_) => continue,
                            }
                        }
                        if had_ok {
                            // Advance past the last successfully tokenized byte
                            if let Some(&(_, _, last_end)) = result.last() {
                                tail_offset = last_end - tail_start;
                            } else {
                                tail_offset = tail.len();
                            }
                        } else {
                            // No progress — skip the problematic byte.
                            // Emit it as a bare SingleQuote or DoubleQuote
                            // so the character is not lost.
                            let ch = tail.as_bytes()[tail_offset];
                            if ch == b'\'' {
                                result.push((
                                    Token::SingleQuote,
                                    tail_start + tail_offset,
                                    tail_start + tail_offset + 1,
                                ));
                            } else if ch == b'"' {
                                result.push((
                                    Token::DoubleQuote,
                                    tail_start + tail_offset,
                                    tail_start + tail_offset + 1,
                                ));
                            }
                            tail_offset += 1;
                        }
                    }
                }
            } else {
                result.push((tok, start, end));
            }
        }

        *tokens = result;
    }

    /// Logos 0.15's `#[^\r\n]*` regex can fail to match through `'`
    /// characters, breaking a Comment into a Comment token (ending at
    /// the `'`) followed by adjacent non-newline tokens.  This function
    /// re-merges such fragments back into the Comment.
    pub fn fix_split_comments(input: &str, tokens: &mut Vec<(Token, usize, usize)>) {
        let mut result: Vec<(Token, usize, usize)> = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            let (tok, start, end) = &tokens[i];
            if !matches!(tok, Token::Comment) {
                result.push(tokens[i].clone());
                i += 1;
                continue;
            }
            let mut merge_end = *end;
            // Look ahead: if the next token starts exactly at merge_end
            // and is NOT a Newline/CarriageReturn, it was gobbled out of
            // the comment by the logos bug.  Absorb it and keep going.
            while i + 1 < tokens.len() {
                let (ref ntok, nstart, nend) = tokens[i + 1];
                if nstart != merge_end {
                    break;
                }
                match ntok {
                    Token::Newline | Token::CarriageReturn => break,
                    _ => {
                        merge_end = nend;
                        i += 1;
                    }
                }
            }
            result.push((Token::Comment, *start, merge_end));
            i += 1;
        }
        *tokens = result;
    }

    /// Fix bare SingleQuote/DoubleQuote tokens that should have been
    /// paired into a proper quoted string, but logos failed to match
    /// them because the closing quote was in a different fragment.
    /// Scans forward in the input to find the matching close quote
    /// and replaces the bare token(s) with a proper string token.
    pub fn fix_bare_quotes(input: &str, tokens: &mut Vec<(Token, usize, usize)>) {
        let mut result: Vec<(Token, usize, usize)> = Vec::new();
        let bytes = input.as_bytes();
        let mut i = 0;
        while i < tokens.len() {
            let (ref tok, start, end) = tokens[i];
            let single_span = end - start == 1
                && (*tok == Token::SingleQuote || *tok == Token::DoubleQuote);
            if !single_span {
                result.push(tokens[i].clone());
                i += 1;
                continue;
            }
            let quote_byte = bytes[start];
            // Only process if this is actually a quote character
            if quote_byte != b'\'' && quote_byte != b'"' {
                result.push(tokens[i].clone());
                i += 1;
                continue;
            }
            // Determine which token type to use for the pair
            let open = start;
            // Scan forward in input to find matching close quote
            let mut pos = open + 1;
            if quote_byte == b'\'' {
                // Simple single-quoted string: find next unescaped '
                while pos < input.len() {
                    if bytes[pos] == b'\'' {
                        // Found it -- replace the bare SingleQuote with
                        // a proper SingleQuotedString
                        result.push((Token::SingleQuotedString, open, pos + 1));
                        // Skip any tokens that lie within this span
                        i += 1;
                        while i < tokens.len() && tokens[i].2 <= pos + 1 {
                            i += 1;
                        }
                        break;
                    }
                    if bytes[pos] == b'\\' && pos + 1 < input.len() {
                        pos += 2; // skip escaped char
                    } else {
                        pos += 1;
                    }
                }
                if pos >= input.len() {
                    // No matching close quote found — keep bare
                    result.push(tokens[i].clone());
                    i += 1;
                }
            } else {
                // Double-quoted string: track $(...)/${...} nesting
                let mut p_depth = 0i32;
                let mut b_depth = 0i32;
                let mut bt_depth = 0i32;
                let mut sq_depth = 0i32;
                let mut dq_depth = 0i32;
                while pos < input.len() {
                    match bytes[pos] {
                        b'"' if p_depth == 0 && b_depth == 0 && bt_depth == 0 => {
                            result.push((Token::DoubleQuotedString, open, pos + 1));
                            i += 1;
                            while i < tokens.len() && tokens[i].2 <= pos + 1 {
                                i += 1;
                            }
                            break;
                        }
                        b'"' if p_depth > 0 || bt_depth > 0 => {
                            dq_depth = if dq_depth == 0 { 1 } else { 0 };
                            pos += 1;
                        }
                        b'\\' if pos + 1 < input.len() && sq_depth == 0 => { pos += 2; }
                        b'`' => { bt_depth = if bt_depth == 0 { 1 } else { 0 }; pos += 1; }
                        b'\'' if p_depth > 0 && dq_depth == 0 => { sq_depth = if sq_depth == 0 { 1 } else { 0 }; pos += 1; }
                        b'$' if pos + 1 < input.len() && bytes[pos + 1] == b'(' && sq_depth == 0 => { p_depth += 1; pos += 2; }
                        b'$' if pos + 1 < input.len() && bytes[pos + 1] == b'{' && sq_depth == 0 => { b_depth += 1; pos += 2; }
                        b')' if sq_depth == 0 => { if p_depth > 0 { p_depth -= 1; } pos += 1; }
                        b'}' if sq_depth == 0 => { if b_depth > 0 { b_depth -= 1; } pos += 1; }
                        _ => { pos += 1; }
                    }
                }
                if pos >= input.len() {
                    // No matching close quote — keep bare
                    result.push(tokens[i].clone());
                    i += 1;
                }
            }
        }
        *tokens = result;
    }

    /// Resolve the ambiguous `((` token.
    ///
    /// In bash, `((` can mean either arithmetic evaluation (`(( expr ))`) or
    /// two nested subshells (`( (cmd) ... )`).  We disambiguate by scanning
    /// forward from each `ArithmeticEval` token to find its matching close.
    /// If the matching close is `ArithmeticEvalClose` (`))`), the `((` is a
    /// true arithmetic expression.  If the matching close is via two separate
    /// `ParenClose` tokens, the `((` is two nested subshells and we split it
    /// into two `ParenOpen` tokens.
    fn resolve_double_paren_ambiguity(input: &str, tokens: &mut Vec<(Token, usize, usize)>) {
        let mut result: Vec<(Token, usize, usize)> = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            if tokens[i].0 == Token::ArithmeticEval {
                let (_, start, end) = tokens[i];
                // Scan forward from i+1 to find how this (( closes.
                let mut depth: i32 = 2;
                let mut j = i + 1;
                let mut closed_by_arithmetic = false;
                while j < tokens.len() && depth > 0 {
                    match tokens[j].0 {
                        Token::ArithmeticEval => depth += 2,
                        Token::Arithmetic => depth += 2, // $((
                        Token::DollarParen => depth += 1, // $(
                        Token::ParenOpen => depth += 1,
                        Token::ArithmeticEvalClose => {
                            depth -= 2;
                            if depth <= 0 {
                                closed_by_arithmetic = true;
                            }
                        }
                        Token::ParenClose => depth -= 1,
                        Token::Comment => {
                            // A Comment token may contain `)` characters when `#` appears
                            // inside an arithmetic expression (e.g. 10#x > 5)).
                            // Count `)` in the comment text to adjust depth correctly.
                            let cm_start = tokens[j].1;
                            let cm_end = tokens[j].2;
                            let text = &input[cm_start..cm_end];
                            depth -= text.chars().filter(|&c| c == ')').count() as i32;
                            depth += text.chars().filter(|&c| c == '(').count() as i32;
                            // Check if depth hit zero (closed by comment content)
                            if depth <= 0 {
                                closed_by_arithmetic = true;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                if closed_by_arithmetic {
                    // True arithmetic (( ... )) — keep as ArithmeticEval.
                    result.push(tokens[i].clone());
                } else {
                    // Nested subshells — split into two ParenOpen tokens.
                    result.push((Token::ParenOpen, start, start + 1));
                    result.push((Token::ParenOpen, start + 1, end));
                }
                i += 1;
            } else {
                result.push(tokens[i].clone());
                i += 1;
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
