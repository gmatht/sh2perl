use crate::ast::*;
use crate::lexer::{Lexer, Token};
use crate::parser::commands::Parser;
use crate::parser::errors::ParserError;
use crate::parser::redirects::parse_redirect;
use crate::parser::utilities::ParserUtilities;
use std::collections::{BTreeMap, HashMap};

fn parse_at_prefixed_word(lexer: &mut Lexer) -> Option<Word> {
    if !matches!(lexer.peek(), Some(Token::At)) {
        return None;
    }

    let mut combined = String::new();
    while matches!(
        lexer.peek(),
        Some(Token::At) | Some(Token::Dollar) | Some(Token::Identifier) | Some(Token::Number)
    ) {
        if let Some(text) = lexer.get_current_text() {
            combined.push_str(&text);
            lexer.next();
        } else {
            break;
        }
    }

    if combined.is_empty() {
        None
    } else {
        lexer.skip_inline_whitespace_and_comments();
        Some(Word::Literal(combined, None))
    }
}

fn plain_text_of_word(word: &Word) -> Option<String> {
    match word {
        Word::Literal(text, _) => Some(text.clone()),
        Word::StringInterpolation(interp, _) => {
            let mut text = String::new();
            for part in &interp.parts {
                if let StringPart::Literal(s) = part {
                    text.push_str(s);
                } else {
                    return None;
                }
            }
            Some(text)
        }
        _ => None,
    }
}

fn append_plain_text(word: &mut Word, fragment: &str) -> bool {
    match word {
        Word::Literal(text, _) => {
            text.push_str(fragment);
            true
        }
        Word::StringInterpolation(interp, _) => {
            if let Some(StringPart::Literal(last)) = interp.parts.last_mut() {
                last.push_str(fragment);
            } else {
                interp.parts.push(StringPart::Literal(fragment.to_string()));
            }
            true
        }
        Word::Variable(var_name, _, _) => {
            let mut parts = vec![StringPart::Variable(var_name.clone())];
            parts.push(StringPart::Literal(fragment.to_string()));
            *word = Word::StringInterpolation(StringInterpolation { parts }, None);
            true
        }
        _ => false,
    }
}

/// Convert a parsed `$`-expansion word into interpolation parts. Every shape
/// parse_variable_expansion can return maps to a StringPart — a missed shape
/// would silently DISCARD the consumed expansion (a value-losing bug), so the
/// match must be exhaustive over Word.
fn expansion_into_parts(expansion: Word) -> Option<Vec<StringPart>> {
    match expansion {
        Word::Variable(name, _, _) => Some(vec![StringPart::Variable(name)]),
        Word::Arithmetic(a, _) => Some(vec![StringPart::Arithmetic(a)]),
        Word::ParameterExpansion(pe, _) => Some(vec![StringPart::ParameterExpansion(pe)]),
        Word::MapAccess(name, key, _) => Some(vec![StringPart::MapAccess(name, key)]),
        Word::MapKeys(name, _) => Some(vec![StringPart::MapKeys(name)]),
        Word::MapLength(name, _) => Some(vec![StringPart::MapLength(name)]),
        Word::ArraySlice(name, offset, len, _) => {
            Some(vec![StringPart::ArraySlice(name, offset, len)])
        }
        Word::Arithmetic(a, _) => Some(vec![StringPart::Arithmetic(a)]),
        Word::CommandSubstitution(c, _) => Some(vec![StringPart::CommandSubstitution(c)]),
        Word::StringInterpolation(interp, _) => Some(interp.parts),
        _ => None,
    }
}

/// Merge a parsed `$`-expansion word into the current word as interpolation
/// parts: `x` + `$var` -> StringInterpolation[Literal("x"), Variable("var")].
/// Returns false when the shapes can't be merged (caller breaks the loop).
fn merge_expansion_into_word(word: &mut Word, expansion: Word) -> bool {
    let Some(parts) = expansion_into_parts(expansion) else {
        return false;
    };
    match word {
        Word::Literal(s, _) => {
            let lit = std::mem::take(s);
            let mut new_parts = vec![StringPart::Literal(lit)];
            new_parts.extend(parts);
            *word = Word::StringInterpolation(StringInterpolation { parts: new_parts }, None);
            true
        }
        Word::Variable(name, _, _) => {
            let mut new_parts = vec![StringPart::Variable(name.clone())];
            new_parts.extend(parts);
            *word = Word::StringInterpolation(StringInterpolation { parts: new_parts }, None);
            true
        }
        Word::StringInterpolation(interp, _) => {
            // Coalesce adjacent Literal parts (e.g. SI[Lit("x")] + SI[Lit("y"), Var]).
            if let Some(StringPart::Literal(last)) = interp.parts.last_mut() {
                if let Some(StringPart::Literal(first)) = parts.first() {
                    last.push_str(first);
                    interp.parts.extend(parts.into_iter().skip(1));
                    return true;
                }
            }
            interp.parts.extend(parts);
            true
        }
        _ => false,
    }
}

/// Append a raw token's text to the word (used for glued literal fragments
/// that follow a single-token word like `$$/x` or `'a'-suffix`).
fn append_raw_token_text(lexer: &mut Lexer, word: &mut Word) -> Result<bool, ParserError> {
    if let Some(text) = lexer.get_current_text() {
        let ok = append_plain_text(word, &text);
        lexer.next();
        // Token::Escape: also consume+append the escaped character
        // (mirrors the combine loop's `\x` handling).
        if text == "\\" {
            if let Some(escaped_text) = lexer.get_current_text() {
                let ok2 = append_plain_text(word, &escaped_text);
                lexer.next();
                return Ok(ok && ok2);
            }
        }
        return Ok(ok);
    }
    Ok(false)
}

fn merge_contiguous_quoted_fragments(
    lexer: &mut Lexer,
    word: &mut Word,
) -> Result<(), ParserError> {
    loop {
        let prev_end = match lexer.tokens.get(lexer.current.saturating_sub(1)) {
            Some((_, _, end)) => *end,
            None => break,
        };
        let next_start = match lexer.tokens.get(lexer.current) {
            Some((_, start, _)) => *start,
            None => break,
        };

        if next_start != prev_end {
            break;
        }

        let fragment = match lexer.peek() {
            Some(Token::SingleQuotedString) => {
                let text = lexer.get_string_text()?;
                strip_outer_quotes(&text)
            }
            Some(Token::DoubleQuotedString) => {
                let fragment_word = parse_string_interpolation(lexer)?;
                match plain_text_of_word(&fragment_word) {
                    Some(text) => text,
                    None => {
                        // The DoubleQuotedString contains variables or
                        // other non-plain-text parts.  Merge the parsed
                        // parts into the current word.
                        let word_literal = match word {
                            Word::Literal(s, _) => Some(s.clone()),
                            Word::StringInterpolation(interp, _) => {
                                let mut s = String::new();
                                for p in &interp.parts {
                                    if let StringPart::Literal(t) = p {
                                        s.push_str(t);
                                    }
                                }
                                if s.is_empty() {
                                    None
                                } else {
                                    Some(s)
                                }
                            }
                            _ => None,
                        };
                        if let Word::StringInterpolation(frag_interp, _) = fragment_word {
                            let mut new_parts = Vec::new();
                            if let Some(lit) = word_literal {
                                new_parts.push(StringPart::Literal(lit));
                            }
                            new_parts.extend(frag_interp.parts);
                            *word = Word::StringInterpolation(
                                StringInterpolation { parts: new_parts },
                                None,
                            );
                        }
                        // We consumed the fragment but cannot represent it
                        // as a plain string.  Break out so the merged word
                        // (now a StringInterpolation) is returned as-is.
                        break;
                    }
                }
            }
            Some(Token::DollarSingleQuotedString) => match parse_ansic_quoted_string(lexer)? {
                Word::Literal(text, _) => text,
                _ => break,
            },
            // `$`-expansions glued to the word (no whitespace): `x$$`, `x$var`,
            // `x${y}`, `x$?`, ... — merge as interpolation parts.
            Some(Token::Dollar) => {
                let is_var_ref = lexer
                    .peek_n(1)
                    .map(|t| matches!(t, Token::Identifier | Token::Number))
                    .unwrap_or(false);
                if !is_var_ref {
                    // literal `$` (e.g. `'a'$/x`) — append the raw char
                    if !append_raw_token_text(lexer, word)? {
                        break;
                    }
                    continue;
                }
                let expansion = parse_variable_expansion(lexer)?;
                if !merge_expansion_into_word(word, expansion) {
                    break;
                }
                continue;
            }
            Some(Token::DollarBrace)
            | Some(Token::DollarParen)
            | Some(Token::DollarHashSimple)
            | Some(Token::DollarAtSimple)
            | Some(Token::DollarStarSimple)
            | Some(Token::DollarQuestion)
            | Some(Token::DollarDollar)
            | Some(Token::DollarBang)
            | Some(Token::DollarMinus)
            | Some(Token::DollarBraceHash)
            | Some(Token::DollarBraceBang)
            | Some(Token::DollarBraceStar)
            | Some(Token::DollarBraceAt)
            | Some(Token::DollarBraceHashStar)
            | Some(Token::DollarBraceHashAt)
            | Some(Token::DollarBraceBangStar)
            | Some(Token::DollarBraceBangAt) => {
                let expansion = parse_variable_expansion(lexer)?;
                if !merge_expansion_into_word(word, expansion) {
                    break;
                }
                continue;
            }
            // `$(( ... ))` glued to the word (`x=$((1+2))` — one bash word):
            // the lexer emits a distinct Arithmetic token the combine loop
            // stops at, so merge the parsed arithmetic as a part here.
            Some(Token::Arithmetic) | Some(Token::ArithmeticEval) => {
                if let Ok(arith) = parse_arithmetic_expression(lexer) {
                    if !merge_expansion_into_word(word, arith) {
                        break;
                    }
                    continue;
                }
                break;
            }
            // Escape fragments (`\'`, `\"`, `\x`) glued after a word:
            // keep the raw text — the renderers unescape it (like `echo a\'b`).
            Some(Token::Escape)
            | Some(Token::EscapedDoubleQuote)
            | Some(Token::EscapedSingleQuote)
            | Some(Token::EscapedBacktick) => {
                if !append_raw_token_text(lexer, word)? {
                    break;
                }
                continue;
            }
            // Literal continuation after a single-token word: `$$/x`, `$var/x`,
            // `'a'-suffix`. Same token set the combine loop treats as
            // word-continuation characters (dead in branch A — the loop already
            // consumed them; live for branch B results).
            Some(Token::Identifier)
            | Some(Token::Number)
            | Some(Token::Float)
            | Some(Token::PaddedNumber)
            | Some(Token::HexNumber)
            | Some(Token::Slash)
            | Some(Token::Dot)
            | Some(Token::Range)
            | Some(Token::Plus)
            | Some(Token::Minus)
            | Some(Token::Colon)
            | Some(Token::Star)
            | Some(Token::Percent)
            | Some(Token::Comma)
            | Some(Token::Question)
            | Some(Token::BraceClose)
            | Some(Token::TestBracket)
            | Some(Token::TestBracketClose)
            | Some(Token::Equality)
            | Some(Token::Caret)
            | Some(Token::PlusAssign)
            | Some(Token::MinusAssign)
            | Some(Token::StarAssign)
            | Some(Token::SlashAssign)
            | Some(Token::PercentAssign)
            | Some(Token::Assign) => {
                if !append_raw_token_text(lexer, word)? {
                    break;
                }
                continue;
            }
            _ => break,
        };

        if !append_plain_text(word, &fragment) {
            break;
        }
    }

    Ok(())
}

fn strip_outer_quotes(text: &str) -> String {
    if (text.starts_with('"') && text.ends_with('"'))
        || (text.starts_with('\'') && text.ends_with('\''))
    {
        text[1..text.len() - 1].to_string()
    } else {
        text.to_string()
    }
}

/// CRLF line endings (Windows-style scripts): bash treats `\r` as a
/// LITERAL word character — a CR at the end of a line JOINS the last word
/// (`echo "hello world"\r\n` prints `hello world\r`); it is not a line
/// terminator. The lexer emits a CarriageReturn token that the parser
/// otherwise treats as a line separator; when one directly follows a word
/// (no whitespace between), append its text to the word so the emitted
/// string matches bash byte-for-byte (crlf-line-endings.sh,
/// parse-crlf-shebang.sh). A CR after whitespace/comments or in other
/// positions keeps its line-separator treatment.
fn append_adjacent_cr(lexer: &mut Lexer, mut word: Word) -> Result<Word, ParserError> {
    if !matches!(lexer.peek(), Some(Token::CarriageReturn)) {
        return Ok(word);
    }
    // The CR is adjacent iff it starts exactly where the word's last real
    // token ended (walk back over any inline whitespace/comments the word
    // parser consumed after the word).
    let mut i = lexer.current;
    while i > 0 {
        match lexer.tokens.get(i - 1).map(|(t, _, _)| t) {
            Some(Token::Space | Token::Tab | Token::Comment) => i -= 1,
            _ => break,
        }
    }
    let adjacent = i > 0 && lexer.tokens[i - 1].2 == lexer.tokens[lexer.current].1;
    if !adjacent {
        return Ok(word);
    }
    let text = lexer.get_string_text()?; // consume the CR token ("\r")
    match &mut word {
        Word::Literal(s, _) => s.push_str(&text),
        Word::StringInterpolation(interp, _) => {
            interp.parts.push(StringPart::Literal(text));
        }
        _ => {}
    }
    Ok(word)
}

pub fn parse_word(lexer: &mut Lexer) -> Result<Word, ParserError> {
    let w = parse_word_inner(lexer)?;
    append_adjacent_cr(lexer, w)
}

fn parse_word_inner(lexer: &mut Lexer) -> Result<Word, ParserError> {
    // Handle backtick command substitution first
    if matches!(lexer.peek(), Some(Token::BacktickChar)) {
        if crate::debug::is_debug_enabled() {
            eprintln!("DEBUG: Found backtick in parse_word");
        }
        lexer.next(); // consume the opening backtick
        let mut cmd_content = String::new();
        while let Some(token) = lexer.peek() {
            match token {
                Token::BacktickChar => {
                    lexer.next(); // consume the closing backtick
                    break;
                }
                _ => {
                    if let Some(text) = lexer.get_current_text() {
                        cmd_content.push_str(&text);
                    }
                    lexer.next();
                }
            }
        }
        if crate::debug::is_debug_enabled() {
            eprintln!("DEBUG: Backtick content: '{}'", cmd_content);
        }
        // Parse the command content
        match crate::parser::commands::parse_pipeline_from_text(&cmd_content) {
            Ok(command) => {
                if crate::debug::is_debug_enabled() {
                    eprintln!("DEBUG: Successfully parsed backtick command: {:?}", command);
                }
                return Ok(Word::CommandSubstitution(Box::new(command), None));
            }
            Err(e) => {
                eprintln!(
                    "DEBUG: Failed to parse backtick command '{}': {:?}",
                    cmd_content, e
                );
                return Ok(Word::Literal(format!("`{}`", cmd_content), None));
            }
        }
    }

    if let Some(word) = parse_at_prefixed_word(lexer) {
        return Ok(word);
    }

    // Combine contiguous bare-word tokens (identifiers, numbers, slashes, dots, plus, minus, colons,
    // and compound assignment operators like +=) into a single literal.
    // This handles filenames like "file.txt" by combining Identifier + Dot + Identifier
    // and also handles find arguments like "+1M" by combining Plus + Number + Identifier
    // and let arguments like "bits+=${#val}" by combining Identifier + PlusAssign + ...
    if matches!(
        lexer.peek(),
        Some(Token::Identifier)
            | Some(Token::Number)
            | Some(Token::Float)
            | Some(Token::PaddedNumber)
            | Some(Token::HexNumber)
            | Some(Token::Slash)
            | Some(Token::Dot)
            | Some(Token::Range)
            | Some(Token::Plus)
            | Some(Token::Minus)
            | Some(Token::Escape)
            | Some(Token::EscapedDoubleQuote) | Some(Token::EscapedSingleQuote) | Some(Token::EscapedBacktick)
            | Some(Token::Colon)
            | Some(Token::Star)
            | Some(Token::Colon)
// Test-operator tokens are intentionally included so combined

            | Some(Token::Colon)
// short flags (`rm -rf`, `echo -rf`) re-join into ONE literal

            | Some(Token::Colon)
// instead of lexing as `-r` + `f`. The lexer emits them as

            | Some(Token::Colon)
// distinct tokens for the test-expression parsers (`[ -f x ]`),

            | Some(Token::Colon)
// which consume them directly; here in argument position the

            | Some(Token::Colon)
// whitespace in the source is the discriminator — `-rf` combines,

            | Some(Token::Colon)
// `-r f` stays two args, exactly like bash. (History: before this,

            | Some(Token::Colon)
// `rm -rf x` and `rm -r f x` parsed identically and rm.rs had a

            | Some(Token::Colon)
// workaround that conflated them, eating a real file named `f`.)

            | Some(Token::Colon)
| Some(Token::Eq) | Some(Token::Ne) | Some(Token::Lt) | Some(Token::Le)

            | Some(Token::Colon)
| Some(Token::Gt) | Some(Token::Ge) | Some(Token::Zero) | Some(Token::NonZero)

            | Some(Token::Colon)
| Some(Token::File) | Some(Token::Directory) | Some(Token::Exists)

            | Some(Token::Colon)
| Some(Token::Readable) | Some(Token::Writable) | Some(Token::Executable)

            | Some(Token::Colon)
| Some(Token::Size) | Some(Token::Symlink) | Some(Token::SymlinkH)

            | Some(Token::Colon)
| Some(Token::PipeFile) | Some(Token::Socket) | Some(Token::Block)

            | Some(Token::Colon)
| Some(Token::Character) | Some(Token::SetGid) | Some(Token::Sticky)

            | Some(Token::Colon)
| Some(Token::SetUid) | Some(Token::Owned) | Some(Token::GroupOwned)

            | Some(Token::Colon)
| Some(Token::Modified) | Some(Token::NewerThan) | Some(Token::OlderThan)

            | Some(Token::Colon)
| Some(Token::SameFile)

            | Some(Token::Percent)
            | Some(Token::Comma)
            | Some(Token::Question)
            | Some(Token::BraceClose)
            | Some(Token::TestBracket)
            | Some(Token::TestBracketClose)
            | Some(Token::Equality)
            | Some(Token::Caret)
            | Some(Token::PlusAssign)
            | Some(Token::MinusAssign)
            | Some(Token::StarAssign)
            | Some(Token::SlashAssign)
            | Some(Token::PercentAssign)
            | Some(Token::Assign)
            // Keywords that can appear in argument position (e.g. `dd if=/dev/zero`)
            | Some(Token::If) | Some(Token::Then) | Some(Token::Else) | Some(Token::Elif)
            | Some(Token::Fi) | Some(Token::Do) | Some(Token::Done)
            | Some(Token::While) | Some(Token::Until) | Some(Token::For)
            | Some(Token::Case) | Some(Token::Esac) | Some(Token::In)
            | Some(Token::Select) | Some(Token::Function) | Some(Token::Source)
    ) {
        let mut combined = String::new();
        loop {
            match lexer.peek() {
                Some(Token::Identifier)
                | Some(Token::Number)
                | Some(Token::Float)
                | Some(Token::PaddedNumber)
                | Some(Token::HexNumber)
                | Some(Token::Slash)
                | Some(Token::Dot)
                | Some(Token::Range)
                | Some(Token::Plus)
                | Some(Token::Minus)
                | Some(Token::Escape)
                | Some(Token::EscapedDoubleQuote) | Some(Token::EscapedSingleQuote) | Some(Token::EscapedBacktick)
                | Some(Token::Colon)
                | Some(Token::Star)
                | Some(Token::Colon)
// Test-operator tokens are intentionally included so combined

                | Some(Token::Colon)
// short flags (`rm -rf`, `echo -rf`) re-join into ONE literal

                | Some(Token::Colon)
// instead of lexing as `-r` + `f`. The lexer emits them as

                | Some(Token::Colon)
// distinct tokens for the test-expression parsers (`[ -f x ]`),

                | Some(Token::Colon)
// which consume them directly; here in argument position the

                | Some(Token::Colon)
// whitespace in the source is the discriminator — `-rf` combines,

                | Some(Token::Colon)
// `-r f` stays two args, exactly like bash. (History: before this,

                | Some(Token::Colon)
// `rm -rf x` and `rm -r f x` parsed identically and rm.rs had a

                | Some(Token::Colon)
// workaround that conflated them, eating a real file named `f`.)

                | Some(Token::Colon)
| Some(Token::Eq) | Some(Token::Ne) | Some(Token::Lt) | Some(Token::Le)

                | Some(Token::Colon)
| Some(Token::Gt) | Some(Token::Ge) | Some(Token::Zero) | Some(Token::NonZero)

                | Some(Token::Colon)
| Some(Token::File) | Some(Token::Directory) | Some(Token::Exists)

                | Some(Token::Colon)
| Some(Token::Readable) | Some(Token::Writable) | Some(Token::Executable)

                | Some(Token::Colon)
| Some(Token::Size) | Some(Token::Symlink) | Some(Token::SymlinkH)

                | Some(Token::Colon)
| Some(Token::PipeFile) | Some(Token::Socket) | Some(Token::Block)

                | Some(Token::Colon)
| Some(Token::Character) | Some(Token::SetGid) | Some(Token::Sticky)

                | Some(Token::Colon)
| Some(Token::SetUid) | Some(Token::Owned) | Some(Token::GroupOwned)

                | Some(Token::Colon)
| Some(Token::Modified) | Some(Token::NewerThan) | Some(Token::OlderThan)

                | Some(Token::Colon)
| Some(Token::SameFile)

                | Some(Token::Percent)
                | Some(Token::Comma)
                | Some(Token::Question)
                | Some(Token::BraceClose)
                | Some(Token::TestBracket)
                | Some(Token::TestBracketClose)
                | Some(Token::Assign)
                | Some(Token::Dollar)
                // Keywords that can appear in argument position
                | Some(Token::If) | Some(Token::Then) | Some(Token::Else) | Some(Token::Elif)
                | Some(Token::Fi) | Some(Token::Do) | Some(Token::Done)
                | Some(Token::While) | Some(Token::Until) | Some(Token::For)
                | Some(Token::Case) | Some(Token::Esac) | Some(Token::In)
                | Some(Token::Select) | Some(Token::Function) | Some(Token::Source)
                => {
                    // For $, check if the NEXT token is a variable name
                    // (Identifier or Number). If so, break out so that
                    // parse_variable_expansion handles the variable reference.
                    if matches!(lexer.peek(), Some(Token::Dollar)) {
                        let is_var_ref = lexer.peek_n(1).map(|t| {
                            matches!(t, Token::Identifier | Token::Number)
                        }).unwrap_or(false);
                        if is_var_ref {
                            break;
                        }
                    }
                    // Append raw token text and consume
                    if let Some(text) = lexer.get_current_text() {
                        combined.push_str(&text);
                        lexer.next();
                        // If this was an escape character, also consume and append
                        // the escaped character that follows (e.g. \$ -> literal $)
                        if matches!(text.as_str(), "\\") {
                            if let Some(escaped_text) = lexer.get_current_text() {
                                combined.push_str(&escaped_text);
                                lexer.next();
                            }
                        }
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
        // If the next token is a BraceOpen, merge the combined literal as
        // a prefix of the brace expansion so that `*.{txt,log,dat}` becomes
        // a BraceExpansion with prefix "*." and items ["txt","log","dat"].
        if matches!(lexer.peek(), Some(Token::BraceOpen)) {
            let brace_word = parse_brace_expansion(lexer)?;
            if let Word::BraceExpansion(mut be, _) = brace_word {
                be.prefix = Some(combined);
                // After the closing brace, consume any immediately adjacent
                // literal text (Identifier, Number, Dot, etc.) as the suffix.
                // This handles `{a,b}suf` where the trailing literal is
                // not followed by another brace expansion.
                let mut suffix = String::new();
                while let Some(tok) = lexer.peek() {
                    match tok {
                        Token::Identifier
                        | Token::Number
                        | Token::Float
                        | Token::PaddedNumber
                        | Token::HexNumber
                        | Token::Slash
                        | Token::Dot
                        | Token::Range
                        | Token::Plus
                        | Token::Minus
                        | Token::Escape
                        | Token::Colon
                        | Token::Star
                        | Token::Percent
                        | Token::Comma
                        | Token::Question
                        | Token::BraceClose
                        | Token::TestBracket
                        | Token::TestBracketClose
                        | Token::Assign
                        | Token::Dollar => {
                            // Stop at Dollar if followed by a variable name
                            // (that would be a new variable expansion)
                            if matches!(tok, Token::Dollar) {
                                let is_var_ref = lexer
                                    .peek_n(1)
                                    .map(|t| matches!(t, Token::Identifier | Token::Number))
                                    .unwrap_or(false);
                                if is_var_ref {
                                    break;
                                }
                            }
                            if let Some(text) = lexer.get_current_text() {
                                suffix.push_str(&text);
                                lexer.next();
                                // Handle escape sequences like in the merge loop
                                if matches!(text.as_str(), "\\") {
                                    if let Some(escaped_text) = lexer.get_current_text() {
                                        suffix.push_str(&escaped_text);
                                        lexer.next();
                                    }
                                }
                            } else {
                                break;
                            }
                        }
                        _ => break,
                    }
                }
                if !suffix.is_empty() {
                    be.suffix = Some(suffix);
                }
                return Ok(Word::BraceExpansion(be, None));
            }
        }

        // Check for immediately adjacent quoted fragments (no whitespace)
        // before skipping whitespace, so `of="$tmpf"` is merged into one word.
        let mut word = Word::Literal(combined, None);
        merge_contiguous_quoted_fragments(lexer, &mut word)?;
        // Skip inline whitespace after consuming the word
        lexer.skip_inline_whitespace_and_comments();
        return Ok(word);
    }

    let result = match lexer.peek() {
        Some(Token::Identifier) => Ok(Word::Literal(lexer.get_identifier_text()?, None)),
        Some(Token::Number) => Ok(Word::Literal(lexer.get_number_text()?, None)),
        Some(Token::Float) => Ok(Word::Literal(lexer.get_raw_token_text()?, None)),
        Some(Token::PaddedNumber) => Ok(Word::Literal(lexer.get_raw_token_text()?, None)),
        Some(Token::HexNumber) => Ok(Word::Literal(lexer.get_raw_token_text()?, None)),
        Some(Token::DoubleQuote) => {
            // Handle a bare DoubleQuote that logos could not match as a full
            // DoubleQuotedString (e.g. because of backslash-newline continuation
            // inside the string). Scan forward through the raw input bytes to
            // find the matching closing quote, resolving backslash-newlines.
            let whole = lexer.scan_double_quoted_string()?;
            // Create a temporary lexer from the cleaned string and parse it
            // as a normal double-quoted string.
            let mut sub_lexer = Lexer::new(&whole);
            if let Some(Token::DoubleQuotedString) = sub_lexer.peek() {
                Ok(parse_string_interpolation(&mut sub_lexer)?)
            } else {
                // Fallback: return the content as a literal
                let inner = if whole.len() >= 2 && whole.as_bytes()[0] == b'"' {
                    &whole[1..]
                } else {
                    &whole
                };
                let inner = if inner.ends_with('"') {
                    &inner[..inner.len() - 1]
                } else {
                    inner
                };
                Ok(Word::Literal(inner.to_string(), None))
            }
        }
        Some(Token::DoubleQuotedString) => {
            // Always parse as string interpolation for double-quoted strings
            // This handles both strings and strings with variables
            Ok(parse_string_interpolation(lexer)?)
        }
        Some(Token::SingleQuotedString) => {
            let quoted_text = lexer.get_string_text()?;
            // Strip the outer quotes from single-quoted strings
            let content = if quoted_text.starts_with("'") && quoted_text.ends_with("'") {
                quoted_text[1..quoted_text.len() - 1].to_string()
            } else {
                quoted_text
            };
            Ok(Word::Literal(content, Some(())))
        }
        Some(Token::SingleQuote) => {
            // Handle a bare single-quote token that wasn't paired into
            // a SingleQuotedString by the lexer (e.g. after heredoc
            // re-tokenization or multi-line single-quoted strings).
            // Scan forward through the raw input to find the matching
            // closing quote.
            let cur = lexer.current;
            let start = if let Some((_, s, _)) = lexer.tokens.get(cur) {
                *s
            } else {
                let pos = lexer.current_position();
                let (line, col) = lexer.offset_to_line_col(pos);
                return Err(ParserError::UnexpectedToken {
                    token: Token::SingleQuote,
                    line,
                    col,
                });
            };
            let bytes = lexer.input.as_bytes();
            let mut pos = start + 1;
            while pos < bytes.len() && bytes[pos] != b'\'' {
                pos += 1;
            }
            let content = if pos < bytes.len() {
                // Found matching close quote
                let end = pos + 1;
                // Advance lexer past all tokens covered by this span
                while lexer.current < lexer.tokens.len() && lexer.tokens[lexer.current].2 <= end {
                    lexer.current += 1;
                }
                lexer.input[start + 1..pos].to_string()
            } else {
                // No matching close quote found - treat the rest as literal
                lexer.current = lexer.tokens.len();
                lexer.input[start + 1..].to_string()
            };
            Ok(Word::Literal(content, Some(())))
        }
        Some(Token::BacktickString) => parse_backtick_command_substitution(lexer),
        Some(Token::DollarSingleQuotedString) => Ok(parse_ansic_quoted_string(lexer)?),
        Some(Token::DollarDoubleQuotedString) => Ok(parse_string_interpolation(lexer)?),
        Some(Token::BraceOpen) => {
            let mut be_word = parse_brace_expansion(lexer)?;
            // After the closing brace, consume immediately adjacent
            // literal text as suffix (e.g. `{a,b}suf`).
            if let Word::BraceExpansion(ref mut be, _) = be_word {
                let mut suffix = String::new();
                while let Some(tok) = lexer.peek() {
                    match tok {
                        Token::Identifier
                        | Token::Number
                        | Token::Float
                        | Token::PaddedNumber
                        | Token::HexNumber
                        | Token::Slash
                        | Token::Dot
                        | Token::Range
                        | Token::Plus
                        | Token::Minus
                        | Token::Escape
                        | Token::Colon
                        | Token::Star
                        | Token::Percent
                        | Token::Comma
                        | Token::Question
                        | Token::BraceClose
                        | Token::TestBracket
                        | Token::TestBracketClose
                        | Token::Assign
                        | Token::Dollar => {
                            if matches!(tok, Token::Dollar) {
                                let is_var_ref = lexer
                                    .peek_n(1)
                                    .map(|t| matches!(t, Token::Identifier | Token::Number))
                                    .unwrap_or(false);
                                if is_var_ref {
                                    break;
                                }
                            }
                            if let Some(text) = lexer.get_current_text() {
                                suffix.push_str(&text);
                                lexer.next();
                                if matches!(text.as_str(), "\\") {
                                    if let Some(escaped_text) = lexer.get_current_text() {
                                        suffix.push_str(&escaped_text);
                                        lexer.next();
                                    }
                                }
                            } else {
                                break;
                            }
                        }
                        _ => break,
                    }
                }
                if !suffix.is_empty() {
                    be.suffix = Some(suffix);
                }
            }
            Ok(be_word)
        }
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
        Some(Token::Star) | Some(Token::Percent) => {
            // Treat standalone '*' as a literal (e.g., `ls *`)
            lexer.next();
            Ok(Word::Literal("*".to_string(), None))
        }
        Some(Token::Dot) => {
            // Treat standalone '.' as a literal (e.g., `ls .`)
            lexer.next();
            Ok(Word::Literal(".".to_string(), None))
        }
        Some(Token::TestBracket) => {
            // Treat [...] expressions as literals (case patterns, array subscripts, etc.)
            let mut text = String::from("[");
            lexer.next(); // consume [
            loop {
                match lexer.peek() {
                    Some(Token::TestBracketClose) => {
                        text.push(']');
                        lexer.next();
                        break;
                    }
                    Some(Token::Escape) => {
                        text.push('\\');
                        lexer.next();
                        if let Some(escaped) = lexer.get_current_text() {
                            text.push_str(&escaped);
                            lexer.next();
                        }
                    }
                    _ => {
                        if let Some(t) = lexer.get_current_text() {
                            text.push_str(&t);
                        }
                        lexer.next();
                    }
                }
            }
            Ok(Word::Literal(text, None))
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
            let mut text = lexer.get_raw_token_text()?;
            // If long option ends with =, merge with following quoted string
            // e.g. --long-option="value with spaces" should be one argument
            if text.ends_with('=') {
                if let Some(next) = lexer.peek() {
                    match next {
                        Token::DoubleQuotedString => {
                            let quoted = lexer.get_string_text()?;
                            let inner = if quoted.starts_with('"') && quoted.ends_with('"') {
                                &quoted[1..quoted.len() - 1]
                            } else {
                                &quoted
                            };
                            // If the quoted value contains an expansion
                            // (`--x="${VAR}"`), flattening it into the literal
                            // would lose the variable reference (the generated
                            // Perl then prints `\${VAR}` literally).  Parse the
                            // quoted content as an interpolation word and keep
                            // the prefix as a literal part.
                            if inner.contains('$') {
                                let prefix = text.clone();
                                if let Ok(interp) =
                                    parse_string_interpolation_from_literal(inner)
                                {
                                    let mut parts = vec![StringPart::Literal(prefix)];
                                    parts.extend(interp.parts);
                                    return Ok(Word::StringInterpolation(
                                        StringInterpolation { parts },
                                        None,
                                    ));
                                }
                            }
                            text.push_str(inner);
                        }
                        Token::SingleQuotedString => {
                            let quoted = lexer.get_string_text()?;
                            let inner = if quoted.starts_with('\'') && quoted.ends_with('\'') {
                                &quoted[1..quoted.len() - 1]
                            } else {
                                &quoted
                            };
                            text.push_str(inner);
                        }
                        _ => {}
                    }
                }
            } else {
                // Strip quotes from value if the regex captured them as part of the token
                // (e.g. --option="value" or --option='value')
                if let Some(eq_pos) = text.find('=') {
                    let value_part = &text[eq_pos + 1..];
                    if value_part.len() >= 2 {
                        if (value_part.starts_with('"') && value_part.ends_with('"'))
                            || (value_part.starts_with('\'') && value_part.ends_with('\''))
                        {
                            let inner = &value_part[1..value_part.len() - 1];
                            // `--x="${X}"` — the quoted value holds an expansion;
                            // keep it as a real interpolation so the generated
                            // Perl substitutes $X instead of printing `\${X}`.
                            if inner.contains('$') {
                                if let Ok(interp) =
                                    parse_string_interpolation_from_literal(inner)
                                {
                                    let mut parts =
                                        vec![StringPart::Literal(format!("{}=", &text[..eq_pos]))];
                                    parts.extend(interp.parts);
                                    return Ok(Word::StringInterpolation(
                                        StringInterpolation { parts },
                                        None,
                                    ));
                                }
                            }
                            text = format!("{}={}", &text[..eq_pos], inner);
                        }
                    }
                }
            }
            Ok(Word::Literal(text, None))
        }
        Some(Token::RegexPattern) => {
            // Treat regex patterns as literals
            let text = lexer.get_raw_token_text()?;
            Ok(Word::Literal(text, None))
        }
        Some(Token::RegexMatch) => {
            // Treat regex match operator as literal
            let text = lexer.get_raw_token_text()?;
            Ok(Word::Literal(text, None))
        }
        Some(Token::NameFlag) | Some(Token::MaxDepthFlag) | Some(Token::TypeFlag) => {
            // Treat command-line flags as literals
            let text = lexer.get_raw_token_text()?;
            Ok(Word::Literal(text, None))
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
        Some(Token::Assign)
        | Some(Token::Character)
        | Some(Token::NonZero)
        | Some(Token::SymlinkH)
        | Some(Token::PipeFile)
        | Some(Token::Socket)
        | Some(Token::Block)
        | Some(Token::SetGid)
        | Some(Token::Sticky)
        | Some(Token::SetUid)
        | Some(Token::Owned)
        | Some(Token::GroupOwned)
        | Some(Token::Modified)
        | Some(Token::Eq)
        | Some(Token::Ne)
        | Some(Token::Lt)
        | Some(Token::Le)
        | Some(Token::Gt)
        | Some(Token::Ge)
        | Some(Token::Zero)
        | Some(Token::SameFile)
        | Some(Token::NewerThan)
        | Some(Token::OlderThan) => {
            // Handle test operator tokens like -e, -f, -d, -ef, -nt, -ot, etc.
            // These are already complete flags, just get their text
            let text = lexer.get_raw_token_text()?;
            Ok(Word::Literal(text, None))
        }
        Some(Token::Dollar) => Ok(parse_variable_expansion(lexer)?),
        Some(Token::DollarBrace)
        | Some(Token::DollarParen)
        | Some(Token::DollarHashSimple)
        | Some(Token::DollarAtSimple)
        | Some(Token::DollarStarSimple)
        | Some(Token::DollarQuestion)
        | Some(Token::DollarDollar)
        | Some(Token::DollarBang)
        | Some(Token::DollarMinus)
        | Some(Token::DollarBraceHash)
        | Some(Token::DollarBraceBang)
        | Some(Token::DollarBraceStar)
        | Some(Token::DollarBraceAt)
        | Some(Token::DollarBraceHashStar)
        | Some(Token::DollarBraceHashAt)
        | Some(Token::DollarBraceBangStar)
        | Some(Token::DollarBraceBangAt) => Ok(parse_variable_expansion(lexer)?),
        Some(Token::Arithmetic) | Some(Token::ArithmeticEval) => {
            Ok(parse_arithmetic_expression(lexer)?)
        }
        Some(Token::ArithmeticBracket) => Ok(parse_arithmetic_bracket(lexer)?),
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
        Some(Token::ParenOpen) => {
            let text = lexer.capture_parenthetical_text()?;
            Ok(Word::Literal(text, None))
        }
        token => {
            // If we encounter a shell keyword token in argument position,
            // treat it as a literal word rather than failing.
            match token {
                Some(Token::If)
                | Some(Token::Then)
                | Some(Token::Else)
                | Some(Token::Elif)
                | Some(Token::Fi)
                | Some(Token::Do)
                | Some(Token::Done)
                | Some(Token::While)
                | Some(Token::Until)
                | Some(Token::For)
                | Some(Token::Case)
                | Some(Token::Esac)
                | Some(Token::In)
                | Some(Token::Select)
                | Some(Token::Function)
                | Some(Token::Bang)
                | Some(Token::Let)
                | Some(Token::Break)
                | Some(Token::Continue)
                | Some(Token::Return)
                | Some(Token::Exit)
                | Some(Token::Shift)
                | Some(Token::Eval)
                | Some(Token::Exec)
                | Some(Token::Source)
                | Some(Token::Trap)
                | Some(Token::Wait)
                | Some(Token::Unset)
                | Some(Token::Set)
                | Some(Token::Export)
                | Some(Token::Readonly)
                | Some(Token::Declare)
                | Some(Token::Typeset)
                | Some(Token::Local) => {
                    let text = lexer.get_current_text().unwrap_or_default();
                    lexer.next();
                    Ok(Word::Literal(text, None))
                }
                Some(Token::Question) => {
                    // A standalone ? in word position is a valid glob character
                    lexer.next();
                    Ok(Word::Literal("?".to_string(), None))
                }
                _ => {
                    let token = token.unwrap_or(Token::Identifier);
                    // Use the actual byte offset of the current token for error position
                    if let Some((_, start, _)) = lexer.tokens.get(lexer.current) {
                        let (line, col) = lexer.offset_to_line_col(*start);
                        Err(ParserError::UnexpectedToken { token, line, col })
                    } else {
                        let current_pos = lexer.current_position();
                        let (line, col) = lexer.offset_to_line_col(current_pos);
                        Err(ParserError::UnexpectedToken { token, line, col })
                    }
                }
            }
        }
    };

    let mut result = result?;
    merge_contiguous_quoted_fragments(lexer, &mut result)?;

    // Skip inline whitespace after consuming the word
    lexer.skip_inline_whitespace_and_comments();

    Ok(result)
}

/// Parse a word without skipping newlines at the end.
/// This is used specifically for argument parsing where we want to preserve newlines.
pub fn parse_word_no_newline_skip(lexer: &mut Lexer) -> Result<Word, ParserError> {
    let w = parse_word_no_newline_skip_inner(lexer)?;
    append_adjacent_cr(lexer, w)
}

fn parse_word_no_newline_skip_inner(lexer: &mut Lexer) -> Result<Word, ParserError> {
    if let Some(word) = parse_at_prefixed_word(lexer) {
        return Ok(word);
    }

    // When parsing command arguments, preserve token boundaries between
    // whitespace-separated words. Adjacent quoted fragments are only merged
    // by parse_word(), which is used in contexts that need that behavior.
    let start_pos = lexer.current_position();

    // Combine contiguous bare-word tokens (identifiers, numbers, slashes, dots, plus, minus, colons,
    // and compound assignment operators like +=) into a single literal.
    // This handles filenames like "file.txt" by combining Identifier + Dot + Identifier
    // and also handles find arguments like "+1M" by combining Plus + Number + Identifier
    // and let arguments like "bits+=${#val}" by combining Identifier + PlusAssign + ...
    if matches!(
        lexer.peek(),
        Some(Token::Identifier)
            | Some(Token::Number)
            | Some(Token::Float)
            | Some(Token::PaddedNumber)
            | Some(Token::HexNumber)
            | Some(Token::Slash)
            | Some(Token::Dot)
            | Some(Token::Range)
            | Some(Token::Plus)
            | Some(Token::Minus)
            | Some(Token::Escape)
            | Some(Token::EscapedDoubleQuote) | Some(Token::EscapedSingleQuote) | Some(Token::EscapedBacktick)
            | Some(Token::Colon)
            | Some(Token::Star)
            | Some(Token::Colon)
// Test-operator tokens are intentionally included so combined

            | Some(Token::Colon)
// short flags (`rm -rf`, `echo -rf`) re-join into ONE literal

            | Some(Token::Colon)
// instead of lexing as `-r` + `f`. The lexer emits them as

            | Some(Token::Colon)
// distinct tokens for the test-expression parsers (`[ -f x ]`),

            | Some(Token::Colon)
// which consume them directly; here in argument position the

            | Some(Token::Colon)
// whitespace in the source is the discriminator — `-rf` combines,

            | Some(Token::Colon)
// `-r f` stays two args, exactly like bash. (History: before this,

            | Some(Token::Colon)
// `rm -rf x` and `rm -r f x` parsed identically and rm.rs had a

            | Some(Token::Colon)
// workaround that conflated them, eating a real file named `f`.)

            | Some(Token::Colon)
| Some(Token::Eq) | Some(Token::Ne) | Some(Token::Lt) | Some(Token::Le)

            | Some(Token::Colon)
| Some(Token::Gt) | Some(Token::Ge) | Some(Token::Zero) | Some(Token::NonZero)

            | Some(Token::Colon)
| Some(Token::File) | Some(Token::Directory) | Some(Token::Exists)

            | Some(Token::Colon)
| Some(Token::Readable) | Some(Token::Writable) | Some(Token::Executable)

            | Some(Token::Colon)
| Some(Token::Size) | Some(Token::Symlink) | Some(Token::SymlinkH)

            | Some(Token::Colon)
| Some(Token::PipeFile) | Some(Token::Socket) | Some(Token::Block)

            | Some(Token::Colon)
| Some(Token::Character) | Some(Token::SetGid) | Some(Token::Sticky)

            | Some(Token::Colon)
| Some(Token::SetUid) | Some(Token::Owned) | Some(Token::GroupOwned)

            | Some(Token::Colon)
| Some(Token::Modified) | Some(Token::NewerThan) | Some(Token::OlderThan)

            | Some(Token::Colon)
| Some(Token::SameFile)

            | Some(Token::Percent)
            | Some(Token::Comma)
            | Some(Token::Question)
            | Some(Token::BraceClose)
            | Some(Token::TestBracket)
            | Some(Token::TestBracketClose)
            | Some(Token::Equality)
            | Some(Token::Caret)
            | Some(Token::PlusAssign)
            | Some(Token::MinusAssign)
            | Some(Token::StarAssign)
            | Some(Token::SlashAssign)
            | Some(Token::PercentAssign)
            | Some(Token::Assign)
            // Keywords that can appear in argument position (e.g. `dd if=/dev/zero`)
            | Some(Token::If) | Some(Token::Then) | Some(Token::Else) | Some(Token::Elif)
            | Some(Token::Fi) | Some(Token::Do) | Some(Token::Done)
            | Some(Token::While) | Some(Token::Until) | Some(Token::For)
            | Some(Token::Case) | Some(Token::Esac) | Some(Token::In)
            | Some(Token::Select) | Some(Token::Function) | Some(Token::Source)
    ) {
        let mut combined = String::new();
        loop {
            match lexer.peek() {
                Some(Token::Identifier)
                | Some(Token::Number)
                | Some(Token::Float)
                | Some(Token::PaddedNumber)
                | Some(Token::HexNumber)
                | Some(Token::Slash)
                | Some(Token::Dot)
                | Some(Token::Range)
                | Some(Token::Plus)
                | Some(Token::Minus)
                | Some(Token::Escape)
                | Some(Token::EscapedDoubleQuote) | Some(Token::EscapedSingleQuote) | Some(Token::EscapedBacktick)
                | Some(Token::Colon)
                | Some(Token::Star)
                | Some(Token::Colon)
// Test-operator tokens are intentionally included so combined

                | Some(Token::Colon)
// short flags (`rm -rf`, `echo -rf`) re-join into ONE literal

                | Some(Token::Colon)
// instead of lexing as `-r` + `f`. The lexer emits them as

                | Some(Token::Colon)
// distinct tokens for the test-expression parsers (`[ -f x ]`),

                | Some(Token::Colon)
// which consume them directly; here in argument position the

                | Some(Token::Colon)
// whitespace in the source is the discriminator — `-rf` combines,

                | Some(Token::Colon)
// `-r f` stays two args, exactly like bash. (History: before this,

                | Some(Token::Colon)
// `rm -rf x` and `rm -r f x` parsed identically and rm.rs had a

                | Some(Token::Colon)
// workaround that conflated them, eating a real file named `f`.)

                | Some(Token::Colon)
| Some(Token::Eq) | Some(Token::Ne) | Some(Token::Lt) | Some(Token::Le)

                | Some(Token::Colon)
| Some(Token::Gt) | Some(Token::Ge) | Some(Token::Zero) | Some(Token::NonZero)

                | Some(Token::Colon)
| Some(Token::File) | Some(Token::Directory) | Some(Token::Exists)

                | Some(Token::Colon)
| Some(Token::Readable) | Some(Token::Writable) | Some(Token::Executable)

                | Some(Token::Colon)
| Some(Token::Size) | Some(Token::Symlink) | Some(Token::SymlinkH)

                | Some(Token::Colon)
| Some(Token::PipeFile) | Some(Token::Socket) | Some(Token::Block)

                | Some(Token::Colon)
| Some(Token::Character) | Some(Token::SetGid) | Some(Token::Sticky)

                | Some(Token::Colon)
| Some(Token::SetUid) | Some(Token::Owned) | Some(Token::GroupOwned)

                | Some(Token::Colon)
| Some(Token::Modified) | Some(Token::NewerThan) | Some(Token::OlderThan)

                | Some(Token::Colon)
| Some(Token::SameFile)

                | Some(Token::Percent)
                | Some(Token::Comma)
                | Some(Token::Question)
                | Some(Token::BraceClose)
                | Some(Token::TestBracket)
                | Some(Token::TestBracketClose)
                | Some(Token::Equality)
                | Some(Token::Caret)
                | Some(Token::PlusAssign)
                | Some(Token::MinusAssign)
                | Some(Token::StarAssign)
                | Some(Token::SlashAssign)
                | Some(Token::PercentAssign)
                | Some(Token::Assign)
                | Some(Token::Dollar)
                // Keywords that can appear in argument position
                | Some(Token::If) | Some(Token::Then) | Some(Token::Else) | Some(Token::Elif)
                | Some(Token::Fi) | Some(Token::Do) | Some(Token::Done)
                | Some(Token::While) | Some(Token::Until) | Some(Token::For)
                | Some(Token::Case) | Some(Token::Esac) | Some(Token::In)
                | Some(Token::Select) | Some(Token::Function) | Some(Token::Source)
                => {
                    // For $, check if the NEXT token is a variable name
                    // (Identifier or Number). If so, break out so that
                    // parse_variable_expansion handles the variable reference.
                    if matches!(lexer.peek(), Some(Token::Dollar)) {
                        let is_var_ref = lexer.peek_n(1).map(|t| {
                            matches!(t, Token::Identifier | Token::Number)
                        }).unwrap_or(false);
                        if is_var_ref {
                            break;
                        }
                    }
                    // Append raw token text and consume
                    if let Some(text) = lexer.get_current_text() {
                        combined.push_str(&text);
                        lexer.next();
                        // If this was an escape character, also consume and append
                        // the escaped character that follows (e.g. \$ -> literal $)
                        if matches!(text.as_str(), "\\") {
                            if let Some(escaped_text) = lexer.get_current_text() {
                                combined.push_str(&escaped_text);
                                lexer.next();
                            }
                        }
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
        // If the next token is a BraceOpen, merge the combined literal as
        // a prefix of the brace expansion so that `file.{txt,md}` becomes
        // a BraceExpansion with prefix "file." and items ["txt","md"].
        if matches!(lexer.peek(), Some(Token::BraceOpen)) {
            let brace_word = parse_brace_expansion(lexer)?;
            if let Word::BraceExpansion(mut be, _) = brace_word {
                be.prefix = Some(combined);
                // After the closing brace, consume any immediately adjacent
                // literal text (Identifier, Number, Dot, etc.) as the suffix.
                // This handles `{a,b}suf` where the trailing literal is
                // not followed by another brace expansion.
                let mut suffix = String::new();
                while let Some(tok) = lexer.peek() {
                    match tok {
                        Token::Identifier
                        | Token::Number
                        | Token::Float
                        | Token::PaddedNumber
                        | Token::HexNumber
                        | Token::Slash
                        | Token::Dot
                        | Token::Range
                        | Token::Plus
                        | Token::Minus
                        | Token::Escape
                        | Token::Colon
                        | Token::Star
                        | Token::Percent
                        | Token::Comma
                        | Token::Question
                        | Token::BraceClose
                        | Token::TestBracket
                        | Token::TestBracketClose
                        | Token::Assign
                        | Token::Dollar => {
                            // Stop at Dollar if followed by a variable name
                            // (that would be a new variable expansion)
                            if matches!(tok, Token::Dollar) {
                                let is_var_ref = lexer
                                    .peek_n(1)
                                    .map(|t| matches!(t, Token::Identifier | Token::Number))
                                    .unwrap_or(false);
                                if is_var_ref {
                                    break;
                                }
                            }
                            if let Some(text) = lexer.get_current_text() {
                                suffix.push_str(&text);
                                lexer.next();
                                // Handle escape sequences like in the merge loop
                                if matches!(text.as_str(), "\\") {
                                    if let Some(escaped_text) = lexer.get_current_text() {
                                        suffix.push_str(&escaped_text);
                                        lexer.next();
                                    }
                                }
                            } else {
                                break;
                            }
                        }
                        _ => break,
                    }
                }
                if !suffix.is_empty() {
                    be.suffix = Some(suffix);
                }
                return Ok(Word::BraceExpansion(be, None));
            }
        }
        // Check for immediately adjacent quoted fragments (no whitespace)
        // before skipping whitespace, so `of="$tmpf"` is merged into one word.
        let mut word = Word::Literal(combined, None);
        merge_contiguous_quoted_fragments(lexer, &mut word)?;
        // Skip inline whitespace after consuming the word, but NOT newlines
        lexer.skip_inline_whitespace_and_comments();
        return Ok(word);
    }

    let result = match lexer.peek() {
        Some(Token::Identifier) => Ok(Word::Literal(lexer.get_identifier_text()?, None)),
        Some(Token::Number) => Ok(Word::Literal(lexer.get_number_text()?, None)),
        Some(Token::Float) => Ok(Word::Literal(lexer.get_raw_token_text()?, None)),
        Some(Token::PaddedNumber) => Ok(Word::Literal(lexer.get_raw_token_text()?, None)),
        Some(Token::HexNumber) => Ok(Word::Literal(lexer.get_raw_token_text()?, None)),
        Some(Token::DoubleQuote) => {
            // Handle a bare DoubleQuote that logos could not match as a full
            // DoubleQuotedString (e.g. because of backslash-newline continuation
            // inside the string). Scan forward through the raw input bytes to
            // find the matching closing quote, resolving backslash-newlines.
            let whole = lexer.scan_double_quoted_string()?;
            // Create a temporary lexer from the cleaned string and parse it
            // as a normal double-quoted string.
            let mut sub_lexer = Lexer::new(&whole);
            if let Some(Token::DoubleQuotedString) = sub_lexer.peek() {
                Ok(parse_string_interpolation(&mut sub_lexer)?)
            } else {
                // Fallback: return the content as a literal
                let inner = if whole.len() >= 2 && whole.as_bytes()[0] == b'"' {
                    &whole[1..]
                } else {
                    &whole
                };
                let inner = if inner.ends_with('"') {
                    &inner[..inner.len() - 1]
                } else {
                    inner
                };
                Ok(Word::Literal(inner.to_string(), None))
            }
        }
        Some(Token::DoubleQuotedString) => {
            // Always parse as string interpolation for double-quoted strings
            // This handles both simple strings and strings with variables
            Ok(parse_string_interpolation(lexer)?)
        }
        Some(Token::SingleQuotedString) => {
            let quoted_text = lexer.get_string_text()?;
            // Strip the outer quotes from single-quoted strings
            let content = if quoted_text.starts_with("'") && quoted_text.ends_with("'") {
                quoted_text[1..quoted_text.len() - 1].to_string()
            } else {
                quoted_text
            };
            Ok(Word::Literal(content, Some(())))
        }
        Some(Token::SingleQuote) => {
            // Handle a bare single-quote token that wasn't paired into
            // a SingleQuotedString by the lexer (e.g. after heredoc
            // re-tokenization or multi-line single-quoted strings).
            // Scan forward through the raw input to find the matching
            // closing quote.
            let cur = lexer.current;
            let start = if let Some((_, s, _)) = lexer.tokens.get(cur) {
                *s
            } else {
                let pos = lexer.current_position();
                let (line, col) = lexer.offset_to_line_col(pos);
                return Err(ParserError::UnexpectedToken {
                    token: Token::SingleQuote,
                    line,
                    col,
                });
            };
            let bytes = lexer.input.as_bytes();
            let mut pos = start + 1;
            while pos < bytes.len() && bytes[pos] != b'\'' {
                pos += 1;
            }
            let content = if pos < bytes.len() {
                // Found matching close quote
                let end = pos + 1;
                // Advance lexer past all tokens covered by this span
                while lexer.current < lexer.tokens.len() && lexer.tokens[lexer.current].2 <= end {
                    lexer.current += 1;
                }
                lexer.input[start + 1..pos].to_string()
            } else {
                // No matching close quote found - treat the rest as literal
                lexer.current = lexer.tokens.len();
                lexer.input[start + 1..].to_string()
            };
            Ok(Word::Literal(content, Some(())))
        }
        Some(Token::BacktickString) => parse_backtick_command_substitution(lexer),
        Some(Token::DollarSingleQuotedString) => Ok(parse_ansic_quoted_string(lexer)?),
        Some(Token::DollarDoubleQuotedString) => Ok(parse_string_interpolation(lexer)?),
        Some(Token::BraceOpen) => {
            let mut be_word = parse_brace_expansion(lexer)?;
            // After the closing brace, consume immediately adjacent
            // literal text as suffix (e.g. `{a,b}suf`).
            if let Word::BraceExpansion(ref mut be, _) = be_word {
                let mut suffix = String::new();
                while let Some(tok) = lexer.peek() {
                    match tok {
                        Token::Identifier
                        | Token::Number
                        | Token::Float
                        | Token::PaddedNumber
                        | Token::HexNumber
                        | Token::Slash
                        | Token::Dot
                        | Token::Range
                        | Token::Plus
                        | Token::Minus
                        | Token::Escape
                        | Token::Colon
                        | Token::Star
                        | Token::Percent
                        | Token::Comma
                        | Token::Question
                        | Token::BraceClose
                        | Token::TestBracket
                        | Token::TestBracketClose
                        | Token::Assign
                        | Token::Dollar => {
                            if matches!(tok, Token::Dollar) {
                                let is_var_ref = lexer
                                    .peek_n(1)
                                    .map(|t| matches!(t, Token::Identifier | Token::Number))
                                    .unwrap_or(false);
                                if is_var_ref {
                                    break;
                                }
                            }
                            if let Some(text) = lexer.get_current_text() {
                                suffix.push_str(&text);
                                lexer.next();
                                if matches!(text.as_str(), "\\") {
                                    if let Some(escaped_text) = lexer.get_current_text() {
                                        suffix.push_str(&escaped_text);
                                        lexer.next();
                                    }
                                }
                            } else {
                                break;
                            }
                        }
                        _ => break,
                    }
                }
                if !suffix.is_empty() {
                    be.suffix = Some(suffix);
                }
            }
            Ok(be_word)
        }
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
        Some(Token::Star) | Some(Token::Percent) | Some(Token::Question) => {
            // Treat standalone '*' as a literal (e.g., `ls *`)
            lexer.next();
            let raw = lexer.get_current_text().unwrap_or_default();
            let text = if raw.is_empty() { "*".to_string() } else { raw };
            Ok(Word::Literal(text, None))
        }
        Some(Token::Dot) => {
            // Treat standalone '.' as a literal (e.g., `ls .`)
            lexer.next();
            Ok(Word::Literal(".".to_string(), None))
        }
        Some(Token::TestBracket) => {
            // Treat [...] expressions as literals (case patterns, array subscripts, etc.)
            let mut text = String::from("[");
            lexer.next(); // consume [
            loop {
                match lexer.peek() {
                    Some(Token::TestBracketClose) => {
                        text.push(']');
                        lexer.next();
                        break;
                    }
                    Some(Token::Escape) => {
                        text.push('\\');
                        lexer.next();
                        if let Some(escaped) = lexer.get_current_text() {
                            text.push_str(&escaped);
                            lexer.next();
                        }
                    }
                    _ => {
                        if let Some(t) = lexer.get_current_text() {
                            text.push_str(&t);
                        }
                        lexer.next();
                    }
                }
            }
            Ok(Word::Literal(text, None))
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
            let mut text = lexer.get_raw_token_text()?;
            // If long option ends with =, merge with following quoted string
            // e.g. --long-option="value with spaces" should be one argument
            if text.ends_with('=') {
                if let Some(next) = lexer.peek() {
                    match next {
                        Token::DoubleQuotedString => {
                            let quoted = lexer.get_string_text()?;
                            let inner = if quoted.starts_with('"') && quoted.ends_with('"') {
                                &quoted[1..quoted.len() - 1]
                            } else {
                                &quoted
                            };
                            // If the quoted value contains an expansion
                            // (`--x="${VAR}"`), flattening it into the literal
                            // would lose the variable reference (the generated
                            // Perl then prints `\${VAR}` literally).  Parse the
                            // quoted content as an interpolation word and keep
                            // the prefix as a literal part.
                            if inner.contains('$') {
                                let prefix = text.clone();
                                if let Ok(interp) =
                                    parse_string_interpolation_from_literal(inner)
                                {
                                    let mut parts = vec![StringPart::Literal(prefix)];
                                    parts.extend(interp.parts);
                                    return Ok(Word::StringInterpolation(
                                        StringInterpolation { parts },
                                        None,
                                    ));
                                }
                            }
                            text.push_str(inner);
                        }
                        Token::SingleQuotedString => {
                            let quoted = lexer.get_string_text()?;
                            let inner = if quoted.starts_with('\'') && quoted.ends_with('\'') {
                                &quoted[1..quoted.len() - 1]
                            } else {
                                &quoted
                            };
                            text.push_str(inner);
                        }
                        _ => {}
                    }
                }
            } else {
                // Strip quotes from value if the regex captured them as part of the token
                // (e.g. --option="value" or --option='value')
                if let Some(eq_pos) = text.find('=') {
                    let value_part = &text[eq_pos + 1..];
                    if value_part.len() >= 2 {
                        if (value_part.starts_with('"') && value_part.ends_with('"'))
                            || (value_part.starts_with('\'') && value_part.ends_with('\''))
                        {
                            let inner = &value_part[1..value_part.len() - 1];
                            // `--x="${X}"` — the quoted value holds an expansion;
                            // keep it as a real interpolation so the generated
                            // Perl substitutes $X instead of printing `\${X}`.
                            if inner.contains('$') {
                                if let Ok(interp) =
                                    parse_string_interpolation_from_literal(inner)
                                {
                                    let mut parts =
                                        vec![StringPart::Literal(format!("{}=", &text[..eq_pos]))];
                                    parts.extend(interp.parts);
                                    return Ok(Word::StringInterpolation(
                                        StringInterpolation { parts },
                                        None,
                                    ));
                                }
                            }
                            text = format!("{}={}", &text[..eq_pos], inner);
                        }
                    }
                }
            }
            Ok(Word::Literal(text, None))
        }
        Some(Token::RegexPattern) => {
            // Treat regex patterns as literals
            let text = lexer.get_raw_token_text()?;
            Ok(Word::Literal(text, None))
        }
        Some(Token::RegexMatch) => {
            // Treat regex match operator as literal
            let text = lexer.get_raw_token_text()?;
            Ok(Word::Literal(text, None))
        }
        Some(Token::NameFlag) | Some(Token::MaxDepthFlag) | Some(Token::TypeFlag) => {
            // Treat command-line flags as literals
            let text = lexer.get_raw_token_text()?;
            Ok(Word::Literal(text, None))
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
        Some(Token::Assign)
        | Some(Token::Character)
        | Some(Token::NonZero)
        | Some(Token::SymlinkH)
        | Some(Token::PipeFile)
        | Some(Token::Socket)
        | Some(Token::Block)
        | Some(Token::SetGid)
        | Some(Token::Sticky)
        | Some(Token::SetUid)
        | Some(Token::Owned)
        | Some(Token::GroupOwned)
        | Some(Token::Modified)
        | Some(Token::Eq)
        | Some(Token::Ne)
        | Some(Token::Lt)
        | Some(Token::Le)
        | Some(Token::Gt)
        | Some(Token::Ge)
        | Some(Token::Zero)
        | Some(Token::SameFile)
        | Some(Token::NewerThan)
        | Some(Token::OlderThan) => {
            // Handle test operator tokens like -e, -f, -d, -ef, -nt, -ot, etc.
            // These are already complete flags, just get their text
            let text = lexer.get_raw_token_text()?;
            Ok(Word::Literal(text, None))
        }
        Some(Token::Dollar) => Ok(parse_variable_expansion(lexer)?),
        Some(Token::DollarBrace)
        | Some(Token::DollarParen)
        | Some(Token::DollarHashSimple)
        | Some(Token::DollarAtSimple)
        | Some(Token::DollarStarSimple)
        | Some(Token::DollarQuestion)
        | Some(Token::DollarDollar)
        | Some(Token::DollarBang)
        | Some(Token::DollarMinus)
        | Some(Token::DollarBraceHash)
        | Some(Token::DollarBraceBang)
        | Some(Token::DollarBraceStar)
        | Some(Token::DollarBraceAt)
        | Some(Token::DollarBraceHashStar)
        | Some(Token::DollarBraceHashAt)
        | Some(Token::DollarBraceBangStar)
        | Some(Token::DollarBraceBangAt) => Ok(parse_variable_expansion(lexer)?),
        Some(Token::Arithmetic) | Some(Token::ArithmeticEval) => {
            Ok(parse_arithmetic_expression(lexer)?)
        }
        Some(Token::ArithmeticBracket) => Ok(parse_arithmetic_bracket(lexer)?),
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
        Some(Token::ParenOpen) => {
            let text = lexer.capture_parenthetical_text()?;
            Ok(Word::Literal(text, None))
        }
        token => {
            // If we encounter a shell keyword token in argument position,
            // treat it as a literal word rather than failing.
            match token {
                Some(Token::If)
                | Some(Token::Then)
                | Some(Token::Else)
                | Some(Token::Elif)
                | Some(Token::Fi)
                | Some(Token::Do)
                | Some(Token::Done)
                | Some(Token::While)
                | Some(Token::Until)
                | Some(Token::For)
                | Some(Token::Case)
                | Some(Token::Esac)
                | Some(Token::In)
                | Some(Token::Select)
                | Some(Token::Function)
                | Some(Token::Bang)
                | Some(Token::Let)
                | Some(Token::Break)
                | Some(Token::Continue)
                | Some(Token::Return)
                | Some(Token::Exit)
                | Some(Token::Shift)
                | Some(Token::Eval)
                | Some(Token::Exec)
                | Some(Token::Source)
                | Some(Token::Trap)
                | Some(Token::Wait)
                | Some(Token::Unset)
                | Some(Token::Set)
                | Some(Token::Export)
                | Some(Token::Readonly)
                | Some(Token::Declare)
                | Some(Token::Typeset)
                | Some(Token::Local) => {
                    let text = lexer.get_current_text().unwrap_or_default();
                    lexer.next();
                    Ok(Word::Literal(text, None))
                }
                _ => {
                    let token = token.unwrap_or(Token::Identifier);
                    // Use the actual byte offset of the current token for error position
                    if let Some((_, start, _)) = lexer.tokens.get(lexer.current) {
                        let (line, col) = lexer.offset_to_line_col(*start);
                        Err(ParserError::UnexpectedToken { token, line, col })
                    } else {
                        let current_pos = lexer.current_position();
                        let (line, col) = lexer.offset_to_line_col(current_pos);
                        Err(ParserError::UnexpectedToken { token, line, col })
                    }
                }
            }
        }
    };

    let mut result = result?;
    if lexer.current_position() != start_pos {
        merge_contiguous_quoted_fragments(lexer, &mut result)?;
    }

    // Don't skip inline whitespace after consuming the word - this preserves newlines
    // for argument parsing context

    Ok(result)
}

pub fn parse_variable_expansion(lexer: &mut Lexer) -> Result<Word, ParserError> {
    match lexer.peek() {
        Some(Token::Dollar) => {
            lexer.next();
            if let Some(Token::Identifier) = lexer.peek() {
                // Variable names in shell can only contain alphanumeric characters
                // and underscores.  The identifier token may include extra characters
                // like `-`, `*`, `?` (e.g. `$CODESET-*`).  Extract only the valid
                // variable name prefix BEFORE calling get_identifier_text (which
                // advances past the token).  Modify the token span so the suffix
                // characters become separate tokens for the merge block.
                let var_name = if let Some(text) = lexer.get_current_text() {
                    text
                } else {
                    return Err(ParserError::InvalidSyntax(
                        "Failed to get identifier text".to_string(),
                    ));
                };
                let valid_var_end = var_name
                    .bytes()
                    .position(|b| !b.is_ascii_alphanumeric() && b != b'_')
                    .unwrap_or(var_name.len());
                if valid_var_end < var_name.len() {
                    let valid_name = &var_name[..valid_var_end];
                    let suffix = &var_name[valid_var_end..];
                    // Truncate the current identifier token to only the valid variable name
                    if let Some((_, start, end)) = lexer.tokens.get_mut(lexer.current) {
                        *end = *start + valid_var_end;
                    }
                    // Insert synthetic tokens for each remaining suffix byte
                    if let Some((_, start, _)) = lexer.tokens.get(lexer.current) {
                        let suffix_start = *start + valid_var_end;
                        for (i, byte) in suffix.bytes().enumerate() {
                            let tok = match byte {
                                b'-' => Token::Minus,
                                b'*' => Token::Star,
                                b'?' => Token::Question,
                                b'.' => Token::Dot,
                                b'/' => Token::Slash,
                                b':' => Token::Colon,
                                _ => Token::Identifier,
                            };
                            lexer.tokens.insert(
                                lexer.current + 1 + i,
                                (tok, suffix_start + i, suffix_start + i + 1),
                            );
                        }
                    }
                    // Now consume the (now-truncated) identifier token
                    lexer.next();
                    return Ok(Word::Variable(valid_name.to_string(), false, None));
                }
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

                // Check for adjacent suffix tokens (no whitespace gap) that
                // should be concatenated, like $DEST.new or $var-suffix.
                if let Some(next_start) = lexer.tokens.get(lexer.current).map(|(_, s, _)| *s) {
                    let prev_end = lexer
                        .tokens
                        .get(lexer.current.checked_sub(1).unwrap_or(0))
                        .map(|(_, _, e)| *e)
                        .unwrap_or(0);
                    if next_start == prev_end {
                        if let Some(Token::Dot) = lexer.peek() {
                            // $var.suffix — consume . and following identifier
                            let mut parts = vec![StringPart::Variable(var_name.clone())];
                            lexer.next(); // consume the Dot
                            if let Some(Token::Identifier) = lexer.peek() {
                                if let Some(id_text) = lexer.get_current_text() {
                                    parts.push(StringPart::Literal(format!(".{}", id_text)));
                                    lexer.next();
                                    return Ok(Word::StringInterpolation(
                                        StringInterpolation { parts },
                                        None,
                                    ));
                                }
                            }
                            // Just the dot
                            parts.push(StringPart::Literal(".".to_string()));
                            return Ok(Word::StringInterpolation(
                                StringInterpolation { parts },
                                None,
                            ));
                        }
                        if let Some(Token::Minus) = lexer.peek() {
                            // $var-suffix — consume - and following identifier
                            let mut parts = vec![StringPart::Variable(var_name.clone())];
                            lexer.next(); // consume the Minus
                            if let Some(Token::Identifier) = lexer.peek() {
                                if let Some(id_text) = lexer.get_current_text() {
                                    parts.push(StringPart::Literal(format!("-{}", id_text)));
                                    lexer.next();
                                    return Ok(Word::StringInterpolation(
                                        StringInterpolation { parts },
                                        None,
                                    ));
                                }
                            }
                            parts.push(StringPart::Literal("-".to_string()));
                            return Ok(Word::StringInterpolation(
                                StringInterpolation { parts },
                                None,
                            ));
                        }
                    }
                }

                Ok(Word::Variable(var_name, false, None))
            } else if let Some(Token::Number) = lexer.peek() {
                // Handle special shell variables like $0, $1, $2, etc.
                let var_name = lexer.get_number_text()?;
                Ok(Word::Variable(var_name, false, None))
            } else {
                // After $, also accept keyword tokens that match shell keywords
                // (e.g., $exec, $prog) since they are valid variable names.
                // Fall back to treating any token text as a variable name.
                if let Some(text) = lexer.get_current_text() {
                    let first = text.chars().next().unwrap_or(' ');
                    if text.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
                        let var_name = text;
                        lexer.next();
                        Ok(Word::Variable(var_name, false, None))
                    } else {
                        // $ followed by non-identifier (e.g., $/ in sed s/$$//)
                        // is a literal $ character. Return it as a literal Word.
                        // IMPORTANT: we have NOT consumed the next token yet,
                        // so a subsequent parse_word call will handle it.
                        return Ok(Word::Literal("$".to_string(), None));
                    }
                } else {
                    Err(ParserError::InvalidSyntax(
                        "Expected identifier or number after $".to_string(),
                    ))
                }
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
        Some(Token::DollarQuestion) => {
            lexer.next();
            Ok(Word::Variable("?".to_string(), false, None))
        }
        Some(Token::DollarDollar) => {
            lexer.next();
            Ok(Word::Variable("$".to_string(), false, None))
        }
        Some(Token::DollarBang) => {
            lexer.next();
            Ok(Word::Variable("!".to_string(), false, None))
        }
        Some(Token::DollarMinus) => {
            lexer.next();
            Ok(Word::Variable("-".to_string(), false, None))
        }
        Some(Token::DollarBrace) => {
            // Parse ${...} expansions
            lexer.next(); // consume ${

            // Parse the entire braced content first, then analyze it
            let braced_content = parse_braced_variable_name(lexer)?;
            // Consume the closing } that parse_braced_variable_name leaves unconsumed
            if matches!(lexer.peek(), Some(Token::BraceClose)) {
                lexer.next();
            }

            eprintln!(
                "DEBUG parse_variable_expansion: braced_content='{}'",
                braced_content
            );
            // Check if this is array syntax first
            if braced_content.starts_with('#')
                && braced_content.contains('[')
                && braced_content.contains(']')
            {
                // This is ${#arr[@]} - array length
                if let Some(bracket_start) = braced_content.find('[') {
                    if let Some(_bracket_end) = braced_content.rfind(']') {
                        let array_name = &braced_content[1..bracket_start]; // Remove # prefix
                        return Ok(Word::MapLength(array_name.to_string(), None));
                    }
                }
            } else if braced_content.starts_with('!')
                && braced_content.contains('[')
                && braced_content.contains(']')
            {
                // This is ${!map[@]} - get keys of associative array
                if let Some(bracket_start) = braced_content.find('[') {
                    if let Some(_bracket_end) = braced_content.rfind(']') {
                        let map_name = &braced_content[1..bracket_start]; // Remove ! prefix
                        return Ok(Word::MapKeys(map_name.to_string(), None));
                    }
                }
            } else if braced_content.starts_with('!')
                && (braced_content.ends_with('@') || braced_content.ends_with('*'))
                && !braced_content.contains('[')
                && !braced_content.contains(']')
            {
                // This is ${!prefix@} or ${!prefix*} - indirect expansion.
                // In bash this expands to all variable names starting with prefix.
                // Generate as keys %prefix for Perl.
                let var_name = &braced_content[1..braced_content.len() - 1];
                return Ok(Word::MapKeys(var_name.to_string(), None));
            } else if braced_content.contains("::") {
                // ${var::offset} or ${var::offset:length} - substring syntax
                // with empty offset (defaults to 0). Must check before :-
                // because ::-2 contains :- as a substring.
                let colon_pos = braced_content.find("::").unwrap();
                let var_name = &braced_content[..colon_pos];
                let rest = &braced_content[colon_pos + 2..];
                // rest is the length (since offset is empty = 0)
                return Ok(Word::ParameterExpansion(
                    ParameterExpansion {
                        variable: var_name.to_string(),
                        operator: ParameterExpansionOperator::ArraySlice(
                            "0".to_string(),
                            Some(rest.to_string()),
                        ),
                        is_mutable: true,
                    },
                    None,
                ));
            } else if braced_content.contains(":-") {
                eprintln!(
                    "DEBUG parse_variable_expansion: found :- in braced_content='{}'",
                    braced_content
                );
                // ${var:-default} - use default if var is empty
                let colon_pos = braced_content.find(":-").unwrap();
                let var_name = &braced_content[..colon_pos];
                let default_val = &braced_content[colon_pos + 2..];
                return Ok(Word::ParameterExpansion(
                    ParameterExpansion {
                        variable: var_name.to_string(),
                        operator: ParameterExpansionOperator::DefaultValue(default_val.to_string()),
                        is_mutable: true,
                    },
                    None,
                ));
            } else if braced_content.contains(":=") {
                // ${var:=default} - assign default if var is empty
                let colon_pos = braced_content.find(":=").unwrap();
                let var_name = &braced_content[..colon_pos];
                let default_val = &braced_content[colon_pos + 2..];
                return Ok(Word::ParameterExpansion(
                    ParameterExpansion {
                        variable: var_name.to_string(),
                        operator: ParameterExpansionOperator::AssignDefault(
                            default_val.to_string(),
                        ),
                        is_mutable: true,
                    },
                    None,
                ));
            } else if braced_content.contains(":+") {
                // ${var:+alt} - use alt if var is set and not empty
                let colon_pos = braced_content.find(":+").unwrap();
                let var_name = &braced_content[..colon_pos];
                let alt_val = &braced_content[colon_pos + 2..];
                return Ok(Word::ParameterExpansion(
                    ParameterExpansion {
                        variable: var_name.to_string(),
                        operator: ParameterExpansionOperator::DefaultValue(alt_val.to_string()),
                        is_mutable: true,
                    },
                    None,
                ));
            } else if braced_content.contains(":?") {
                // ${var:?error} - error if var is empty
                let colon_pos = braced_content.find(":?").unwrap();
                let var_name = &braced_content[..colon_pos];
                let error_msg = &braced_content[colon_pos + 2..];
                return Ok(Word::ParameterExpansion(
                    ParameterExpansion {
                        variable: var_name.to_string(),
                        operator: ParameterExpansionOperator::ErrorIfUnset(error_msg.to_string()),
                        is_mutable: true,
                    },
                    None,
                ));
            } else if braced_content.contains(':')
                && !braced_content.contains("::")
                && !braced_content.contains(":-")
                && !braced_content.contains(":=")
                && !braced_content.contains(":+")
                && !braced_content.contains(":?")
            {
                // ${var:offset} or ${var:offset:length} - substring/array-slice
                // The first colon must not be part of any two-char operator.
                let colon_pos = braced_content.find(':').unwrap();
                let var_name = &braced_content[..colon_pos];
                let rest = &braced_content[colon_pos + 1..];
                if let Some(second_colon) = rest.find(':') {
                    let offset = &rest[..second_colon];
                    let length = &rest[second_colon + 1..];
                    return Ok(Word::ParameterExpansion(
                        ParameterExpansion {
                            variable: var_name.to_string(),
                            operator: ParameterExpansionOperator::ArraySlice(
                                offset.to_string(),
                                Some(length.to_string()),
                            ),
                            is_mutable: true,
                        },
                        None,
                    ));
                } else {
                    return Ok(Word::ParameterExpansion(
                        ParameterExpansion {
                            variable: var_name.to_string(),
                            operator: ParameterExpansionOperator::ArraySlice(
                                rest.to_string(),
                                None,
                            ),
                            is_mutable: true,
                        },
                        None,
                    ));
                }
            } else if braced_content.contains('[') && braced_content.contains(']') {
                // This might be a map/array access like ${map[foo]} or ${arr[1]} or ${map[$k]}
                // OR it might be a parameter expansion with brackets in the pattern
                // (e.g. ${0##*[/\]}).  Check if the part before `[` contains pattern
                // operators: if so, fall through to the pattern-operator checks below.
                if let Some(bracket_start) = braced_content.find('[') {
                    if let Some(bracket_end) = braced_content.rfind(']') {
                        let map_name = &braced_content[..bracket_start];
                        let key = &braced_content[bracket_start + 1..bracket_end];

                        // Guard: if the variable-name portion before `[` contains
                        // pattern-removal or substitution operators, this is a
                        // parameter expansion with brackets in the pattern, not
                        // an array/map access.
                        if !(map_name.contains('#')
                            || map_name.contains('%')
                            || map_name.contains('/'))
                        {
                            // Special case: if key is "@", this is array iteration
                            if key == "@" {
                                // Check if there's array slicing in braced_content after ']'
                                let after_bracket = &braced_content[bracket_end + 1..];
                                if after_bracket.starts_with(':') {
                                    // This is array slicing like ${arr[@]:start:length}
                                    let slice_part = &after_bracket[1..]; // skip leading ':'
                                    if let Some(second_colon) = slice_part.find(':') {
                                        let offset = &slice_part[..second_colon];
                                        let length = &slice_part[second_colon + 1..];
                                        return Ok(Word::array_slice(
                                            map_name.to_string(),
                                            offset.to_string(),
                                            Some(length.to_string()),
                                        ));
                                    } else {
                                        return Ok(Word::array_slice(
                                            map_name.to_string(),
                                            slice_part.to_string(),
                                            None,
                                        ));
                                    }
                                }
                                return Ok(Word::MapAccess(
                                    map_name.to_string(),
                                    "@".to_string(),
                                    None,
                                ));
                            }

                            // Trailing junk after `]` in a NON-@ subscript
                            // (e.g. `${arr[1]>2}`): bash rejects the whole
                            // expansion as a "bad substitution" (skips the
                            // command, status 1). A `:` continuation is a
                            // valid element slice (`${arr[1]:0:2}`) — kept
                            // as-is below (pre-existing behavior).
                            let after_bracket = &braced_content[bracket_end + 1..];
                            if !after_bracket.is_empty() && !after_bracket.starts_with(':') {
                                return Ok(Word::ParameterExpansion(
                                    ParameterExpansion {
                                        variable: braced_content.to_string(),
                                        operator: ParameterExpansionOperator::BadSubstitution,
                                        is_mutable: true,
                                    },
                                    None,
                                ));
                            }

                            return Ok(Word::MapAccess(
                                map_name.to_string(),
                                key.to_string(),
                                None,
                            ));
                        }
                        // else: fall through to parameter expansion checks below
                    }
                }
            }

            // Check for parameter expansion operators
            // Note: colon-prefix operators like :- := :+ :? are handled above
            // before the array access check, so only non-colon operators remain here.

            // Check if this is a parameter expansion with operators
            // Check longer patterns first to avoid partial matches
            if braced_content.ends_with("^^") {
                let base_var = braced_content.trim_end_matches("^^");
                Ok(Word::ParameterExpansion(
                    ParameterExpansion {
                        variable: base_var.to_string(),
                        operator: ParameterExpansionOperator::UppercaseAll,
                        is_mutable: true,
                    },
                    None,
                ))
            } else if braced_content.ends_with(",,") {
                let base_var = braced_content.trim_end_matches(",,");
                Ok(Word::ParameterExpansion(
                    ParameterExpansion {
                        variable: base_var.to_string(),
                        operator: ParameterExpansionOperator::LowercaseAll,
                        is_mutable: true,
                    },
                    None,
                ))
            } else if braced_content.ends_with("^") && !braced_content.ends_with("^^") {
                let base_var = braced_content.trim_end_matches("^");
                Ok(Word::ParameterExpansion(
                    ParameterExpansion {
                        variable: base_var.to_string(),
                        operator: ParameterExpansionOperator::UppercaseFirst,
                        is_mutable: true,
                    },
                    None,
                ))
            } else if braced_content.ends_with("##*/")
                // Also guard: the `#` of `#*/` must not be preceded by `#`
                // (otherwise this is actually `${var##pattern}` not `${var##*/}`).
                && !braced_content.ends_with("###*/")
            {
                let base_var = braced_content.trim_end_matches("##*/");
                Ok(Word::ParameterExpansion(
                    ParameterExpansion {
                        variable: base_var.to_string(),
                        operator: ParameterExpansionOperator::Basename,
                        is_mutable: true,
                    },
                    None,
                ))
            } else if braced_content.ends_with("%/*")
                // Guard: the `%` of `%/*` must not be preceded by `%`
                // (otherwise this is actually `${var%%pattern}` not `${var%/*}`).
                && !braced_content.ends_with("%%/*")
            {
                let base_var = braced_content.trim_end_matches("%/*");
                Ok(Word::ParameterExpansion(
                    ParameterExpansion {
                        variable: base_var.to_string(),
                        operator: ParameterExpansionOperator::Dirname,
                        is_mutable: true,
                    },
                    None,
                ))
            } else if braced_content.contains("##") && !braced_content.ends_with("##*/") {
                let parts: Vec<&str> = braced_content.split("##").collect();
                if parts.len() == 2 {
                    Ok(Word::ParameterExpansion(
                        ParameterExpansion {
                            variable: parts[0].to_string(),
                            operator: ParameterExpansionOperator::RemoveLongestPrefix(
                                parts[1].to_string(),
                            ),
                            is_mutable: true,
                        },
                        None,
                    ))
                } else {
                    Ok(Word::Variable(braced_content, true, None))
                }
            } else if braced_content.contains("%%")
                && !(braced_content.ends_with("%/*") && !braced_content.ends_with("%%/*"))
            {
                let parts: Vec<&str> = braced_content.split("%%").collect();
                if parts.len() == 2 {
                    Ok(Word::ParameterExpansion(
                        ParameterExpansion {
                            variable: parts[0].to_string(),
                            operator: ParameterExpansionOperator::RemoveLongestSuffix(
                                parts[1].to_string(),
                            ),
                            is_mutable: true,
                        },
                        None,
                    ))
                } else {
                    Ok(Word::Variable(braced_content, true, None))
                }
            } else if braced_content.contains("#") && !braced_content.contains("##") {
                let parts: Vec<&str> = braced_content.splitn(2, "#").collect();
                if parts.len() == 2 {
                    Ok(Word::ParameterExpansion(
                        ParameterExpansion {
                            variable: parts[0].to_string(),
                            operator: ParameterExpansionOperator::RemoveShortestPrefix(
                                parts[1].to_string(),
                            ),
                            is_mutable: true,
                        },
                        None,
                    ))
                } else {
                    Ok(Word::Variable(braced_content, true, None))
                }
            } else if braced_content.contains("%")
                && !braced_content.contains("%%")
                && !(braced_content.ends_with("%/*") && !braced_content.ends_with("%%/*"))
            {
                let parts: Vec<&str> = braced_content.splitn(2, "%").collect();
                if parts.len() == 2 {
                    Ok(Word::ParameterExpansion(
                        ParameterExpansion {
                            variable: parts[0].to_string(),
                            operator: ParameterExpansionOperator::RemoveShortestSuffix(
                                parts[1].to_string(),
                            ),
                            is_mutable: true,
                        },
                        None,
                    ))
                } else {
                    Ok(Word::Variable(braced_content, true, None))
                }
            } else if braced_content.contains("//") {
                let parts: Vec<&str> = braced_content.split("//").collect();
                if parts.len() == 3 {
                    Ok(Word::ParameterExpansion(
                        ParameterExpansion {
                            variable: parts[0].to_string(),
                            operator: ParameterExpansionOperator::SubstituteAll(
                                parts[1].to_string(),
                                parts[2].to_string(),
                            ),
                            is_mutable: true,
                        },
                        None,
                    ))
                } else {
                    Ok(Word::Variable(braced_content, true, None))
                }
            } else if braced_content.contains("/") && !braced_content.contains("//") {
                let parts: Vec<&str> = braced_content.split("/").collect();
                if parts.len() == 3 {
                    Ok(Word::ParameterExpansion(
                        ParameterExpansion {
                            variable: parts[0].to_string(),
                            operator: ParameterExpansionOperator::SubstituteFirst(
                                parts[1].to_string(),
                                parts[2].to_string(),
                            ),
                            is_mutable: true,
                        },
                        None,
                    ))
                } else {
                    Ok(Word::Variable(braced_content, true, None))
                }
            } else if let Some(minus_pos) = braced_content.find('-') {
                // ${var-default} or ${var-default_value} - use default if var is UNSET (not empty)
                // The first `-` after position 0 is the operator (variable names can't contain hyphens).
                if minus_pos > 0 {
                    let var_name = &braced_content[..minus_pos];
                    let default_val = &braced_content[minus_pos + 1..];
                    Ok(Word::ParameterExpansion(
                        ParameterExpansion {
                            variable: var_name.to_string(),
                            operator: ParameterExpansionOperator::DefaultValue(
                                default_val.to_string(),
                            ),
                            is_mutable: true,
                        },
                        None,
                    ))
                } else {
                    Ok(Word::Variable(braced_content, true, None))
                }
            } else if let Some(plus_pos) = braced_content.find('+') {
                // ${var+alt} or ${var+alt_value} - use alt if var is SET
                if plus_pos > 0 {
                    let var_name = &braced_content[..plus_pos];
                    let alt_val = &braced_content[plus_pos + 1..];
                    Ok(Word::ParameterExpansion(
                        ParameterExpansion {
                            variable: var_name.to_string(),
                            operator: ParameterExpansionOperator::DefaultValue(alt_val.to_string()),
                            is_mutable: true,
                        },
                        None,
                    ))
                } else {
                    Ok(Word::Variable(braced_content, true, None))
                }
            } else if let Some(quest_pos) = braced_content.find('?') {
                // ${var?error} - error if var is UNSET
                if quest_pos > 0 {
                    let var_name = &braced_content[..quest_pos];
                    let error_msg = &braced_content[quest_pos + 1..];
                    Ok(Word::ParameterExpansion(
                        ParameterExpansion {
                            variable: var_name.to_string(),
                            operator: ParameterExpansionOperator::ErrorIfUnset(
                                error_msg.to_string(),
                            ),
                            is_mutable: true,
                        },
                        None,
                    ))
                } else {
                    Ok(Word::Variable(braced_content, true, None))
                }
            } else if let Some(eq_pos) = braced_content.find('=') {
                // ${var=default} - assign default if var is UNSET
                if eq_pos > 0 {
                    let var_name = &braced_content[..eq_pos];
                    let default_val = &braced_content[eq_pos + 1..];
                    Ok(Word::ParameterExpansion(
                        ParameterExpansion {
                            variable: var_name.to_string(),
                            operator: ParameterExpansionOperator::AssignDefault(
                                default_val.to_string(),
                            ),
                            is_mutable: true,
                        },
                        None,
                    ))
                } else {
                    Ok(Word::Variable(braced_content, true, None))
                }
            } else {
                // If it's not a special case, return as a variable
                Ok(Word::Variable(braced_content, true, None))
            }
        }
        Some(Token::DollarBraceHash) => {
            lexer.next();
            let braced_content = parse_braced_variable_name(lexer)?;
            if matches!(lexer.peek(), Some(Token::BraceClose)) {
                lexer.next();
            }
            let prefixed = format!("#{}", braced_content);
            // ${#...} - variable length
            if prefixed.starts_with('#') && prefixed.contains('[') && prefixed.contains(']') {
                // ${#arr[@]} - array length
                Ok(Word::MapLength(
                    braced_content[..braced_content.find('[').unwrap_or(0)].to_string(),
                    None,
                ))
            } else {
                Ok(Word::Variable(prefixed, true, None))
            }
        }
        Some(Token::DollarBraceBang) => {
            lexer.next();
            let braced_content = parse_braced_variable_name(lexer)?;
            if matches!(lexer.peek(), Some(Token::BraceClose)) {
                lexer.next();
            }
            let prefixed = format!("!{}", braced_content);
            // ${!...} - indirect reference or map keys
            if prefixed.starts_with('!') && prefixed.contains('[') && prefixed.contains(']') {
                if let Some(bracket_start) = prefixed.find('[') {
                    if prefixed[bracket_start..].contains('@')
                        || prefixed[bracket_start..].contains('*')
                    {
                        // ${!map[@]} - get keys
                        let map_name = &prefixed[1..bracket_start];
                        return Ok(Word::MapKeys(map_name.to_string(), None));
                    }
                }
            } else if prefixed.starts_with('!')
                && (prefixed.ends_with('@') || prefixed.ends_with('*'))
                && !prefixed.contains('[')
                && !prefixed.contains(']')
            {
                // ${!prefix@} or ${!prefix*} - indirect expansion.
                // In bash this expands to all variable names starting with prefix.
                // Generate as keys %prefix for Perl.
                let var_name = &prefixed[1..prefixed.len() - 1];
                return Ok(Word::MapKeys(var_name.to_string(), None));
            }
            Ok(Word::Variable(prefixed, true, None))
        }
        Some(Token::DollarBraceStar) => {
            lexer.next();
            if matches!(lexer.peek(), Some(Token::BraceClose)) {
                lexer.next();
                Ok(Word::Variable("*".to_string(), true, None))
            } else {
                // ${*...} with additional operators — build content and analyze
                let mut content = String::from("*");
                let rest = parse_braced_variable_name(lexer)?;
                if matches!(lexer.peek(), Some(Token::BraceClose)) {
                    lexer.next();
                }
                content.push_str(&rest);
                // Reuse the same analysis as DollarBraceAt (inline below)
                if content.contains(":-") {
                    let colon_pos = content.find(":-").unwrap();
                    let var_name = &content[..colon_pos];
                    let default_val = &content[colon_pos + 2..];
                    Ok(Word::ParameterExpansion(
                        ParameterExpansion {
                            variable: var_name.to_string(),
                            operator: ParameterExpansionOperator::DefaultValue(
                                default_val.to_string(),
                            ),
                            is_mutable: true,
                        },
                        None,
                    ))
                } else if content.contains(":=") {
                    let colon_pos = content.find(":=").unwrap();
                    let var_name = &content[..colon_pos];
                    let default_val = &content[colon_pos + 2..];
                    Ok(Word::ParameterExpansion(
                        ParameterExpansion {
                            variable: var_name.to_string(),
                            operator: ParameterExpansionOperator::AssignDefault(
                                default_val.to_string(),
                            ),
                            is_mutable: true,
                        },
                        None,
                    ))
                } else if content.contains(":+") {
                    let colon_pos = content.find(":+").unwrap();
                    let var_name = &content[..colon_pos];
                    let alt_val = &content[colon_pos + 2..];
                    Ok(Word::ParameterExpansion(
                        ParameterExpansion {
                            variable: var_name.to_string(),
                            operator: ParameterExpansionOperator::DefaultValue(alt_val.to_string()),
                            is_mutable: true,
                        },
                        None,
                    ))
                } else if let Some(minus_pos) = content.find('-') {
                    if minus_pos > 0 {
                        let var_name = &content[..minus_pos];
                        let default_val = &content[minus_pos + 1..];
                        Ok(Word::ParameterExpansion(
                            ParameterExpansion {
                                variable: var_name.to_string(),
                                operator: ParameterExpansionOperator::DefaultValue(
                                    default_val.to_string(),
                                ),
                                is_mutable: true,
                            },
                            None,
                        ))
                    } else {
                        Ok(Word::Variable(content, true, None))
                    }
                } else if let Some(plus_pos) = content.find('+') {
                    if plus_pos > 0 {
                        let var_name = &content[..plus_pos];
                        let alt_val = &content[plus_pos + 1..];
                        Ok(Word::ParameterExpansion(
                            ParameterExpansion {
                                variable: var_name.to_string(),
                                operator: ParameterExpansionOperator::DefaultValue(
                                    alt_val.to_string(),
                                ),
                                is_mutable: true,
                            },
                            None,
                        ))
                    } else {
                        Ok(Word::Variable(content, true, None))
                    }
                } else if let Some(quest_pos) = content.find('?') {
                    if quest_pos > 0 {
                        let var_name = &content[..quest_pos];
                        let error_msg = &content[quest_pos + 1..];
                        Ok(Word::ParameterExpansion(
                            ParameterExpansion {
                                variable: var_name.to_string(),
                                operator: ParameterExpansionOperator::ErrorIfUnset(
                                    error_msg.to_string(),
                                ),
                                is_mutable: true,
                            },
                            None,
                        ))
                    } else {
                        Ok(Word::Variable(content, true, None))
                    }
                } else if let Some(eq_pos) = content.find('=') {
                    if eq_pos > 0 {
                        let var_name = &content[..eq_pos];
                        let default_val = &content[eq_pos + 1..];
                        Ok(Word::ParameterExpansion(
                            ParameterExpansion {
                                variable: var_name.to_string(),
                                operator: ParameterExpansionOperator::AssignDefault(
                                    default_val.to_string(),
                                ),
                                is_mutable: true,
                            },
                            None,
                        ))
                    } else {
                        Ok(Word::Variable(content, true, None))
                    }
                } else if content.contains(':')
                    && !content.contains("::")
                    && !content.contains(":-")
                    && !content.contains(":=")
                    && !content.contains(":+")
                    && !content.contains(":?")
                {
                    // ${*:offset} or ${*:offset:length} - array slice
                    let colon_pos = content.find(':').unwrap();
                    let var_name = &content[..colon_pos];
                    let rest = &content[colon_pos + 1..];
                    if let Some(second_colon) = rest.find(':') {
                        let offset = &rest[..second_colon];
                        let length = &rest[second_colon + 1..];
                        Ok(Word::ParameterExpansion(
                            ParameterExpansion {
                                variable: var_name.to_string(),
                                operator: ParameterExpansionOperator::ArraySlice(
                                    offset.to_string(),
                                    Some(length.to_string()),
                                ),
                                is_mutable: true,
                            },
                            None,
                        ))
                    } else {
                        Ok(Word::ParameterExpansion(
                            ParameterExpansion {
                                variable: var_name.to_string(),
                                operator: ParameterExpansionOperator::ArraySlice(
                                    rest.to_string(),
                                    None,
                                ),
                                is_mutable: true,
                            },
                            None,
                        ))
                    }
                } else {
                    Ok(Word::Variable(content, true, None))
                }
            }
        }
        Some(Token::DollarBraceAt) => {
            lexer.next();
            eprintln!("DEBUG DollarBraceAt: peek={:?}", lexer.peek());
            if matches!(lexer.peek(), Some(Token::BraceClose)) {
                lexer.next();
                eprintln!("DEBUG DollarBraceAt: just @");
                Ok(Word::Variable("@".to_string(), true, None))
            } else {
                // ${@...} with additional operators — build content and analyze
                let mut content = String::from("@");
                let rest = parse_braced_variable_name(lexer)?;
                if matches!(lexer.peek(), Some(Token::BraceClose)) {
                    lexer.next();
                }
                content.push_str(&rest);
                // Analyze the content using the same logic as the DollarBrace branch
                // (inline analysis, same as lines ~1030-1275)
                if content.contains(":-") {
                    let colon_pos = content.find(":-").unwrap();
                    let var_name = &content[..colon_pos];
                    let default_val = &content[colon_pos + 2..];
                    Ok(Word::ParameterExpansion(
                        ParameterExpansion {
                            variable: var_name.to_string(),
                            operator: ParameterExpansionOperator::DefaultValue(
                                default_val.to_string(),
                            ),
                            is_mutable: true,
                        },
                        None,
                    ))
                } else if content.contains(":=") {
                    let colon_pos = content.find(":=").unwrap();
                    let var_name = &content[..colon_pos];
                    let default_val = &content[colon_pos + 2..];
                    Ok(Word::ParameterExpansion(
                        ParameterExpansion {
                            variable: var_name.to_string(),
                            operator: ParameterExpansionOperator::AssignDefault(
                                default_val.to_string(),
                            ),
                            is_mutable: true,
                        },
                        None,
                    ))
                } else if content.contains(":+") {
                    let colon_pos = content.find(":+").unwrap();
                    let var_name = &content[..colon_pos];
                    let alt_val = &content[colon_pos + 2..];
                    Ok(Word::ParameterExpansion(
                        ParameterExpansion {
                            variable: var_name.to_string(),
                            operator: ParameterExpansionOperator::DefaultValue(alt_val.to_string()),
                            is_mutable: true,
                        },
                        None,
                    ))
                } else if let Some(minus_pos) = content.find('-') {
                    // ${@var-default} - use default if var is UNSET
                    if minus_pos > 0 {
                        let var_name = &content[..minus_pos];
                        let default_val = &content[minus_pos + 1..];
                        Ok(Word::ParameterExpansion(
                            ParameterExpansion {
                                variable: var_name.to_string(),
                                operator: ParameterExpansionOperator::DefaultValue(
                                    default_val.to_string(),
                                ),
                                is_mutable: true,
                            },
                            None,
                        ))
                    } else {
                        Ok(Word::Variable(content, true, None))
                    }
                } else if let Some(plus_pos) = content.find('+') {
                    // ${@var+alt} - use alt if var is SET
                    if plus_pos > 0 {
                        let var_name = &content[..plus_pos];
                        let alt_val = &content[plus_pos + 1..];
                        Ok(Word::ParameterExpansion(
                            ParameterExpansion {
                                variable: var_name.to_string(),
                                operator: ParameterExpansionOperator::DefaultValue(
                                    alt_val.to_string(),
                                ),
                                is_mutable: true,
                            },
                            None,
                        ))
                    } else {
                        Ok(Word::Variable(content, true, None))
                    }
                } else if let Some(quest_pos) = content.find('?') {
                    // ${@var?error} - error if var is UNSET
                    if quest_pos > 0 {
                        let var_name = &content[..quest_pos];
                        let error_msg = &content[quest_pos + 1..];
                        Ok(Word::ParameterExpansion(
                            ParameterExpansion {
                                variable: var_name.to_string(),
                                operator: ParameterExpansionOperator::ErrorIfUnset(
                                    error_msg.to_string(),
                                ),
                                is_mutable: true,
                            },
                            None,
                        ))
                    } else {
                        Ok(Word::Variable(content, true, None))
                    }
                } else if let Some(eq_pos) = content.find('=') {
                    // ${@var=default} - assign default if var is UNSET
                    if eq_pos > 0 {
                        let var_name = &content[..eq_pos];
                        let default_val = &content[eq_pos + 1..];
                        Ok(Word::ParameterExpansion(
                            ParameterExpansion {
                                variable: var_name.to_string(),
                                operator: ParameterExpansionOperator::AssignDefault(
                                    default_val.to_string(),
                                ),
                                is_mutable: true,
                            },
                            None,
                        ))
                    } else {
                        Ok(Word::Variable(content, true, None))
                    }
                } else if content.contains(':')
                    && !content.contains("::")
                    && !content.contains(":-")
                    && !content.contains(":=")
                    && !content.contains(":+")
                    && !content.contains(":?")
                {
                    // ${@:offset} or ${@:offset:length} - array slice
                    let colon_pos = content.find(':').unwrap();
                    let var_name = &content[..colon_pos];
                    let rest = &content[colon_pos + 1..];
                    if let Some(second_colon) = rest.find(':') {
                        let offset = &rest[..second_colon];
                        let length = &rest[second_colon + 1..];
                        Ok(Word::ParameterExpansion(
                            ParameterExpansion {
                                variable: var_name.to_string(),
                                operator: ParameterExpansionOperator::ArraySlice(
                                    offset.to_string(),
                                    Some(length.to_string()),
                                ),
                                is_mutable: true,
                            },
                            None,
                        ))
                    } else {
                        Ok(Word::ParameterExpansion(
                            ParameterExpansion {
                                variable: var_name.to_string(),
                                operator: ParameterExpansionOperator::ArraySlice(
                                    rest.to_string(),
                                    None,
                                ),
                                is_mutable: true,
                            },
                            None,
                        ))
                    }
                } else {
                    Ok(Word::Variable(content, true, None))
                }
            }
        }
        Some(Token::DollarBraceHashStar) => {
            lexer.next();
            if matches!(lexer.peek(), Some(Token::BraceClose)) {
                lexer.next();
            }
            Ok(Word::Variable("#*".to_string(), true, None))
        }
        Some(Token::DollarBraceHashAt) => {
            lexer.next();
            if matches!(lexer.peek(), Some(Token::BraceClose)) {
                lexer.next();
            }
            Ok(Word::Variable("#@".to_string(), true, None))
        }
        Some(Token::DollarBraceBangStar) => {
            lexer.next();
            if matches!(lexer.peek(), Some(Token::BraceClose)) {
                lexer.next();
            }
            Ok(Word::Variable("!*".to_string(), true, None))
        }
        Some(Token::DollarBraceBangAt) => {
            lexer.next();
            if matches!(lexer.peek(), Some(Token::BraceClose)) {
                lexer.next();
            }
            Ok(Word::Variable("!@".to_string(), true, None))
        }
        Some(Token::DollarParen) => {
            // Parse $(...) command substitution
            let command_text = lexer.capture_parenthetical_text()?;
            // Parse the command_text into an actual Command
            let sub_lexer = Lexer::new(&command_text);
            let mut sub_parser = Parser::new_with_lexer(sub_lexer);
            match sub_parser.parse() {
                Ok(commands) => {
                    if commands.is_empty() {
                        // If no commands parsed, treat as a simple command with the text as argument
                        let placeholder_cmd = Command::Simple(SimpleCommand {
                            name: Word::Literal("echo".to_string(), None),
                            args: vec![Word::Literal(command_text, None)],
                            redirects: Vec::new(),
                            env_vars: BTreeMap::new(),
                            stdout_used: true,
                            stderr_used: true,
                        });
                        Ok(Word::CommandSubstitution(Box::new(placeholder_cmd), None))
                    } else if commands.len() == 1 {
                        Ok(Word::CommandSubstitution(
                            Box::new(commands[0].clone()),
                            None,
                        ))
                    } else {
                        // Multiple commands: wrap in a Block so the whole
                        // body runs (a `$(cmd1\ncmd2)` substitution captures
                        // both — parse-dollar-paren-pipe.sh).
                        let block = crate::ast::Block { commands };
                        Ok(Word::CommandSubstitution(
                            Box::new(crate::ast::Command::Block(block)),
                            None,
                        ))
                    }
                }
                Err(_) => {
                    // Fallback: treat as a simple command with the text as argument
                    let placeholder_cmd = Command::Simple(SimpleCommand {
                        name: Word::Literal("echo".to_string(), None),
                        args: vec![Word::Literal(command_text, None)],
                        redirects: Vec::new(),
                        env_vars: BTreeMap::new(),
                        stdout_used: true,
                        stderr_used: true,
                    });
                    Ok(Word::CommandSubstitution(Box::new(placeholder_cmd), None))
                }
            }
        }
        _ => {
            let current_pos = lexer.current_position();
            let (line, col) = lexer.offset_to_line_col(current_pos);
            Err(ParserError::UnexpectedToken {
                token: Token::Identifier,
                line,
                col,
            })
        }
    }
}

// Placeholder functions - these would need to be implemented based on the actual AST structures

fn parse_string_interpolation(lexer: &mut Lexer) -> Result<Word, ParserError> {
    use crate::ast::{StringInterpolation, Word};

    // Get the double-quoted string content (this includes the quotes)
    let string_content = lexer.get_string_text()?;

    // Remove the outer quotes
    let content = if string_content.starts_with('"') && string_content.ends_with('"') {
        &string_content[1..string_content.len() - 1]
    } else {
        &string_content
    };

    let content = unescape_interpolation_content(content);

    if crate::debug::is_debug_enabled() {
        eprintln!(
            "DEBUG parse_string_interpolation: content len={}, content={:?}",
            content.len(),
            &content[..content.len().min(80)]
        );
    }

    let parts = scan_interpolation_parts(&content)?;

    Ok(Word::StringInterpolation(
        StringInterpolation { parts },
        None,
    ))
}

/// The double-quoted-string content preprocessing shared by both
/// interpolation scanners: `\"` → `"`, `\\` → `\`, and backslash-newline
/// line continuations are removed (the lexer's DoubleQuotedString regex
/// captures them inside the token).
fn unescape_interpolation_content(content: &str) -> String {
    content
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
        .replace("\\\n", "")
        .replace("\\\r\n", "")
}

/// Scan a double-quoted string's content into interpolation parts:
/// literal text, `$var` / `${...}` / `$((...))` / `$(...)` / backtick
/// substitutions. Shared by the lexer-driven `parse_string_interpolation`
/// and the LongOption-quoted-value path
/// (`parse_string_interpolation_from_literal` — `--x="${X}"` must keep
/// the expansion as a part, not flatten it into the literal; the corpus
/// test parse-longoption-with-dollar.sh documents this).
fn scan_interpolation_parts(content: &str) -> Result<Vec<StringPart>, ParserError> {
    use crate::ast::{Command, SimpleCommand, StringPart};
    use std::collections::BTreeMap;

    // Parse the string content to extract literal parts and variable references
    let mut parts = Vec::new();
    let mut current_literal = String::new();
    let mut i = 0;

    while i < content.len() {
        let _ch = content.chars().nth(i).unwrap_or('?');
        if content[i..].starts_with("\\\\`") {
            // We found an escaped backtick command substitution
            // First, add any accumulated literal text
            if !current_literal.is_empty() {
                parts.push(StringPart::Literal(current_literal.clone()));
                current_literal.clear();
            }

            // Find the closing escaped backtick
            i += 3; // skip the \\`
            let cmd_start = i;
            while i < content.len() && !content[i..].starts_with("\\\\`") {
                let ch = content[i..].chars().next().unwrap_or('?');
                i += ch.len_utf8();
            }

            if i < content.len() {
                // We found a complete escaped command substitution
                let cmd_content = &content[cmd_start..i];
                i += 3; // skip the closing \\`

                // Parse the command content as a pipeline (to handle pipes)
                if let Ok(cmd) = crate::parser::commands::parse_pipeline_from_text(cmd_content) {
                    parts.push(StringPart::CommandSubstitution(Box::new(cmd)));
                } else {
                    // Fall back to treating it as a literal
                    parts.push(StringPart::Literal(format!("\\\\`{}\\\\`", cmd_content)));
                }
            } else {
                // Unmatched escaped backtick (`\\\`` = escaped backslash
                // + escaped backtick in a DQS): bash consumes BOTH escapes —
                // `\\` → `\` and `\`` → a literal backtick — so the
                // literal text is `\`` (one backslash + backtick), NOT the
                // raw `\\\``. (echo-with-escaped-backtick.sh: top-level
                // DQS `\`` with no closing pair.)
                parts.push(StringPart::Literal("\\`".to_string()));
                i = cmd_start;
            }
        } else if content[i..].starts_with("\\`") {
            // We found a single-escaped backtick command substitution
            // First, add any accumulated literal text
            if !current_literal.is_empty() {
                parts.push(StringPart::Literal(current_literal.clone()));
                current_literal.clear();
            }

            // Find the closing escaped backtick
            i += 2; // skip the \`
            let cmd_start = i;
            while i < content.len() && !content[i..].starts_with("\\`") {
                let ch = content[i..].chars().next().unwrap_or('?');
                i += ch.len_utf8();
            }

            if i < content.len() {
                // We found a complete escaped command substitution
                let cmd_content = &content[cmd_start..i];
                i += 2; // skip the closing \`

                // Parse the command content as a pipeline (to handle pipes)
                if let Ok(cmd) = crate::parser::commands::parse_pipeline_from_text(cmd_content) {
                    parts.push(StringPart::CommandSubstitution(Box::new(cmd)));
                } else {
                    // Fall back to treating it as a literal
                    parts.push(StringPart::Literal(format!("\\`{}\\`", cmd_content)));
                }
            } else {
                // Unmatched single-escaped backtick (a DQS `\\`` with no
                // closing pair): bash consumes the backslash — the result
                // is a literal backtick, NOT `\\`` (echo-with-escaped-
                // backtick.sh + -and-quotes.sh print `Invalid
                // configuration `...`). In a backtick-cmdsub context a
                // MATCHED `\\`...\\`` pair is a nested substitution (the
                // branch above); an unmatched one is a literal backtick
                // in every context.
                parts.push(StringPart::Literal("`".to_string()));
                i = cmd_start;
            }
        } else if content[i..].starts_with("`") {
            // We found a backtick command substitution
            // First, add any accumulated literal text
            if !current_literal.is_empty() {
                parts.push(StringPart::Literal(current_literal.clone()));
                current_literal.clear();
            }

            // Find the closing backtick
            i += 1; // skip the opening `
            let cmd_start = i;
            while i < content.len() && content[i..].chars().next() != Some('`') {
                let ch = content[i..].chars().next().unwrap_or('?');
                i += ch.len_utf8();
            }

            if i < content.len() {
                // We found a complete command substitution
                let cmd_content = &content[cmd_start..i];
                i += 1; // skip the closing `

                // Parse the command content using the full parser to handle pipelines
                let sub_lexer = Lexer::new(cmd_content);
                let mut sub_parser = Parser::new_with_lexer(sub_lexer);
                match sub_parser.parse() {
                    Ok(commands) => {
                        eprintln!(
                            "DEBUG: String interpolation parsed command '{}' as {} commands",
                            cmd_content,
                            commands.len()
                        );
                        if commands.len() == 1 {
                            parts.push(StringPart::CommandSubstitution(Box::new(
                                commands[0].clone(),
                            )));
                        } else if commands.is_empty() {
                            // If no commands parsed, treat as a simple command with the text as argument
                            let placeholder_cmd = Command::Simple(SimpleCommand {
                                name: Word::Literal("echo".to_string(), None),
                                args: vec![Word::Literal(cmd_content.to_string(), None)],
                                redirects: Vec::new(),
                                env_vars: BTreeMap::new(),
                                stdout_used: true,
                                stderr_used: true,
                            });
                            parts.push(StringPart::CommandSubstitution(Box::new(placeholder_cmd)));
                        } else {
                            // If multiple commands, use the first one
                            parts.push(StringPart::CommandSubstitution(Box::new(
                                commands[0].clone(),
                            )));
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "DEBUG: String interpolation failed to parse command '{}': {:?}",
                            cmd_content, e
                        );
                        // Fall back to treating it as a literal
                        parts.push(StringPart::Literal(format!("`{}`", cmd_content)));
                    }
                }
            } else {
                // Unmatched backtick, treat as literal
                parts.push(StringPart::Literal("`".to_string()));
                i = cmd_start;
            }
        } else if i + 1 < content.len() && content[i..].starts_with("\\$") {
            // Escaped dollar sign \$ -> literal $
            current_literal.push('$');
            i += 2;
        } else if content[i..].starts_with("$") {
            // We found a variable reference (or a literal $ that's not a variable)
            // First, add any accumulated literal text
            if !current_literal.is_empty() {
                parts.push(StringPart::Literal(current_literal.clone()));
                current_literal.clear();
            }

            if i + 2 < content.len() && content[i..].starts_with("$((") {
                // `$(( ... ))` arithmetic expansion — find the matching
                // `))` (paren-balanced) and emit an Arithmetic part; before
                // the `$(` branch, or `(1+2)` would parse as a subshell
                // command (`echo "x=$((1+2))"` → running `1+2` as a command).
                let mut j = i + 3;
                let mut depth = 2usize;
                while j < content.len() && depth > 0 {
                    if content.as_bytes()[j] == b'(' {
                        depth += 1;
                    } else if content.as_bytes()[j] == b')' {
                        depth -= 1;
                    }
                    j += 1;
                }
                if depth == 0 {
                    parts.push(StringPart::Arithmetic(ArithmeticExpression {
                        expression: content[i + 3..j - 2].trim().to_string(),
                        tokens: vec![],
                    }));
                    i = j;
                    continue;
                }
                // unbalanced — fall through to the literal `$` handling
            }
            if i + 1 < content.len() && content[i + 1..].starts_with('(') {
                // Command substitution $(...)
                i += 2; // skip $ and (
                let cmd_start = i;
                let mut paren_count = 1;
                // Track single-quote depth inside $(): a ' inside $() starts
                // a single-quoted string where ( ) and $ are literal.
                // Also handle backslash escapes.
                let mut sq_depth = false;
                // Track double-quote depth inside $(): a " inside $() is a
                // double-quoted string where ' is literal and must NOT toggle
                // sq_depth.
                let mut dq_depth = 0i32;
                while i < content.len() && paren_count > 0 {
                    // Check for backslash escape first; skip the escaped char.
                    if content[i..].starts_with('\\') {
                        i += 2;
                        continue;
                    }
                    match content[i..].chars().next() {
                        Some('"') if dq_depth == 0 && !sq_depth => {
                            // Toggle double-quote depth. A `"` inside a
                            // single-quoted string within $() is LITERAL
                            // (bash: `awk '{ print $1 }' | sed 's/"//g'`
                            // inside a DQS) — it must not toggle dq_depth,
                            // or the closing `'` would be misread and the
                            // `)` would never close the substitution
                            // (parse-gaps: multiple-awk-in-dqs.sh).
                            dq_depth = 1;
                        }
                        Some('"') if !sq_depth => {
                            dq_depth = 0;
                        }
                        Some('\'') if dq_depth == 0 => {
                            // Toggle single-quote depth, but only when NOT
                            // inside a double-quoted string within $().
                            sq_depth = !sq_depth;
                        }
                        Some('(') if !sq_depth => paren_count += 1,
                        Some(')') if !sq_depth => paren_count -= 1,
                        _ => {}
                    }
                    let ch = content[i..].chars().next().unwrap_or('?');
                    i += ch.len_utf8();
                }
                if paren_count == 0 {
                    let cmd_content = &content[cmd_start..i - 1];
                    // Multi-command `$(cmd1\ncmd2)` bodies: the pipeline
                    // parser silently drops everything after the first
                    // pipeline (parse-dollar-paren-pipe.sh). Try the full
                    // parser FIRST only when the pipeline parse did not
                    // consume the whole text — a `$(( expr ))` mis-read
                    // (`$( ( expr ) )`) must keep the pipeline-parsed padded
                    // command shape the lowering recovers as arithmetic
                    // (parse-paren-close.sh).
                    match crate::parser::commands::parse_pipeline_from_text_with_rest(cmd_content) {
                        Ok((cmd, true)) => {
                            parts.push(StringPart::CommandSubstitution(Box::new(cmd)));
                        }
                        Ok((_, false)) | Err(_) => {
                            if let Ok(cmds) =
                                crate::parser::commands::parse_commands_from_text(cmd_content)
                            {
                                if cmds.is_empty() {
                                    parts.push(StringPart::Literal(format!("$({})", cmd_content)));
                                } else if cmds.len() == 1 {
                                    parts.push(StringPart::CommandSubstitution(Box::new(
                                        cmds.into_iter().next().unwrap(),
                                    )));
                                } else {
                                    let block = crate::ast::Block { commands: cmds };
                                    parts.push(StringPart::CommandSubstitution(Box::new(
                                        crate::ast::Command::Block(block),
                                    )));
                                }
                            } else {
                                parts.push(StringPart::Literal(format!("$({})", cmd_content)));
                            }
                        }
                    }
                } else {
                    current_literal.push_str("$(");
                    i = cmd_start;
                }
            } else if i + 1 < content.len() && content[i + 1..].starts_with('{') {
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
                    let ch = content[i..].chars().next().unwrap_or('?');
                    i += ch.len_utf8();
                }

                if brace_count == 0 {
                    // We found a complete parameter expansion
                    let expansion_content = &content[expansion_start..i - 1]; // -1 to exclude the closing }

                    // Parse the parameter expansion content
                    if let Ok(expansion_word) = parse_parameter_expansion_content(expansion_content)
                    {
                        parts.push(StringPart::ParameterExpansion(expansion_word));
                    } else {
                        // Fall back to treating it as a literal
                        parts.push(StringPart::Literal(format!("${{{}}}", expansion_content)));
                    }
                } else {
                    // Unmatched braces: bash rejects unterminated `${`
                    // at parse time (exit 2) — a real syntax error, not a
                    // literal (parse-parameter-expansion-eof.sh: the old
                    // literal fallback silently executed the broken word,
                    // bash=2 vs estree=0). The CLI's parse-error fallback
                    // reproduces bash's verdict.
                    return Err(ParserError::InvalidSyntax(
                        "unterminated `${` in double-quoted string".to_string(),
                    ));
                }
            } else {
                // Simple variable reference like $var or a literal $ followed by a non-variable char
                // Skip the $ for inspection, but if it's not a valid variable start we'll treat it as literal
                i += 1; // skip the $

                // If there's no following character, treat the $ as a literal
                if i >= content.len() {
                    current_literal.push('$');
                    break;
                }

                let next_char = content[i..].chars().next().unwrap();
                if next_char == '#'
                    || next_char == '@'
                    || next_char == '*'
                    || next_char == '?'
                    || next_char == '-'
                    || next_char == '!'
                    || next_char == '$'
                {
                    // Special shell variable
                    parts.push(StringPart::Variable(next_char.to_string()));
                    i += 1;
                } else if next_char.is_alphanumeric() || next_char == '_' {
                    // Regular variable name
                    let var_start = i;
                    while i < content.len() {
                        let nc = content[i..].chars().next();
                        if let Some(c) = nc {
                            if c.is_alphanumeric() || c == '_' {
                                let ch = content[i..].chars().next().unwrap_or('?');
                                i += ch.len_utf8();
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
                } else {
                    // Not a recognized variable start; treat the $ as a literal and
                    // leave the following character to be processed in the next loop iteration.
                    current_literal.push('$');
                }
            }
        } else if content[i..].starts_with("$") {
            // Found a $ inside a literal-style string. This could be a variable or a literal $.
            // Treat it similarly to parse_string_interpolation: preserve $ when it's not a valid variable start.
            if !current_literal.is_empty() {
                parts.push(StringPart::Literal(current_literal.clone()));
                current_literal.clear();
            }

            if i + 2 < content.len() && content[i..].starts_with("$((") {
                // `$(( ... ))` arithmetic expansion — find the matching
                // `))` (paren-balanced) and emit an Arithmetic part; before
                // the `$(` branch, or `(1+2)` would parse as a subshell
                // command (`echo "x=$((1+2))"` → running `1+2` as a command).
                let mut j = i + 3;
                let mut depth = 2usize;
                while j < content.len() && depth > 0 {
                    if content.as_bytes()[j] == b'(' {
                        depth += 1;
                    } else if content.as_bytes()[j] == b')' {
                        depth -= 1;
                    }
                    j += 1;
                }
                if depth == 0 {
                    parts.push(StringPart::Arithmetic(ArithmeticExpression {
                        expression: content[i + 3..j - 2].trim().to_string(),
                        tokens: vec![],
                    }));
                    i = j;
                    continue;
                }
                // unbalanced — fall through to the literal `$` handling
            }
            if i + 1 < content.len() && content[i + 1..].starts_with('(') {
                // Command substitution $(...)
                i += 2; // skip $ and (
                let cmd_start = i;
                let mut paren_count = 1;
                while i < content.len() && paren_count > 0 {
                    match content[i..].chars().next() {
                        Some('(') => paren_count += 1,
                        Some(')') => paren_count -= 1,
                        _ => {}
                    }
                    let ch = content[i..].chars().next().unwrap_or('?');
                    i += ch.len_utf8();
                }
                if paren_count == 0 {
                    let cmd_content = &content[cmd_start..i - 1];
                    if crate::debug::is_debug_enabled() {
                        eprintln!("DEBUG parse_string_interpolation: found $(...), cmd_content len={}, first 80: {:?}", cmd_content.len(), &cmd_content[..cmd_content.len().min(80)]);
                    }
                    if let Ok(cmds) = crate::parser::commands::parse_commands_from_text(cmd_content)
                    {
                        if crate::debug::is_debug_enabled() {
                            eprintln!("DEBUG parse_string_interpolation: parse_commands_from_text OK, {} commands", cmds.len());
                        }
                        if cmds.is_empty() {
                            parts.push(StringPart::Literal(format!("$({})", cmd_content)));
                        } else if cmds.len() == 1 {
                            parts.push(StringPart::CommandSubstitution(Box::new(
                                cmds.into_iter().next().unwrap(),
                            )));
                        } else {
                            let block = crate::ast::Block { commands: cmds };
                            parts.push(StringPart::CommandSubstitution(Box::new(
                                crate::ast::Command::Block(block),
                            )));
                        }
                    } else {
                        if crate::debug::is_debug_enabled() {
                            eprintln!("DEBUG parse_string_interpolation: parse_commands_from_text FAILED, trying pipeline");
                        }
                        // Fallback: try the old pipeline-based parser
                        if let Ok(cmd) =
                            crate::parser::commands::parse_pipeline_from_text(cmd_content)
                        {
                            if crate::debug::is_debug_enabled() {
                                eprintln!("DEBUG parse_string_interpolation: pipeline parser OK");
                            }
                            parts.push(StringPart::CommandSubstitution(Box::new(cmd)));
                        } else {
                            if crate::debug::is_debug_enabled() {
                                eprintln!("DEBUG parse_string_interpolation: both parsers failed, using literal");
                            }
                            parts.push(StringPart::Literal(format!("$({})", cmd_content)));
                        }
                    }
                } else {
                    current_literal.push_str("$(");
                    i = cmd_start;
                }
            } else if i + 1 < content.len() && content[i + 1..].starts_with('{') {
                i += 2; // skip $ and {
                let expansion_start = i;
                let mut brace_count = 1;
                while i < content.len() && brace_count > 0 {
                    match content[i..].chars().next() {
                        Some('{') => brace_count += 1,
                        Some('}') => brace_count -= 1,
                        _ => {}
                    }
                    let ch = content[i..].chars().next().unwrap_or('?');
                    i += ch.len_utf8();
                }

                if brace_count == 0 {
                    let expansion_content = &content[expansion_start..i - 1];
                    if let Ok(expansion_word) = parse_parameter_expansion_content(expansion_content)
                    {
                        parts.push(StringPart::ParameterExpansion(expansion_word));
                    } else {
                        parts.push(StringPart::Literal(format!("${{{}}}", expansion_content)));
                    }
                } else {
                    // Unmatched braces: bash rejects unterminated `${`
                    // at parse time (exit 2) — a real syntax error, not a
                    // literal (see the identical arm in
                    // parse_string_interpolation).
                    return Err(ParserError::InvalidSyntax(
                        "unterminated `${` in double-quoted string".to_string(),
                    ));
                }
            } else {
                // Simple $var or special var or literal $
                i += 1; // skip $
                if i >= content.len() {
                    current_literal.push('$');
                    break;
                }

                let next_char = content[i..].chars().next().unwrap();
                if next_char == '#'
                    || next_char == '@'
                    || next_char == '*'
                    || next_char == '?'
                    || next_char == '-'
                    || next_char == '!'
                    || next_char == '$'
                {
                    parts.push(StringPart::Variable(next_char.to_string()));
                    let ch = content[i..].chars().next().unwrap_or('?');
                    i += ch.len_utf8();
                } else if next_char.is_alphanumeric() || next_char == '_' {
                    let var_start = i;
                    while i < content.len() {
                        let nc = content[i..].chars().next();
                        if let Some(c) = nc {
                            if c.is_alphanumeric() || c == '_' {
                                let ch = content[i..].chars().next().unwrap_or('?');
                                i += ch.len_utf8();
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
                } else {
                    // Not a variable - keep the $ as a literal and continue
                    current_literal.push('$');
                }
            }
        } else {
            // Add to current literal
            let ch = content[i..].chars().next().unwrap();
            current_literal.push(ch);
            i += ch.len_utf8();
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

    Ok(parts)
}

/// Parse a literal string as string interpolation — the LongOption lexer
/// path (`--x="${X}"` merges the quoted value into the option text as raw
/// text) and the Perl generator's echo/arg handling re-parse a quoted
/// value that may hold expansions. The FULL scanner runs here (not just
/// backticks): `--x="${X}"` must produce a ParameterExpansion part or the
/// generated code prints `\${X}` literally (parse-longoption-with-dollar.sh). to handle escaped backticks
pub fn parse_string_interpolation_from_literal(
    literal: &str,
) -> Result<StringInterpolation, ParserError> {
    // Remove outer quotes if present
    let content = if (literal.starts_with('"') && literal.ends_with('"'))
        || (literal.starts_with('\'') && literal.ends_with('\''))
    {
        &literal[1..literal.len() - 1]
    } else {
        literal
    };

    let content = unescape_interpolation_content(content);

    let parts = scan_interpolation_parts(&content)?;

    Ok(StringInterpolation { parts })
}

pub fn parse_parameter_expansion_content(content: &str) -> Result<ParameterExpansion, ParserError> {
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

    // Check for substring syntax ${var::length} BEFORE :-
    // because ::-2 would match :-2 incorrectly.
    if content.contains("::") {
        let colon_pos = content.find("::").unwrap();
        let var_name = &content[..colon_pos];
        let rest = &content[colon_pos + 2..];
        return Ok(ParameterExpansion {
            variable: var_name.to_string(),
            operator: ParameterExpansionOperator::ArraySlice(
                "0".to_string(),
                Some(rest.to_string()),
            ),
            is_mutable: true,
        });
    }

    // Check for parameter expansion operators with colon prefix BEFORE array access
    if content.contains(":-") {
        let parts: Vec<&str> = content.splitn(2, ":-").collect();
        if parts.len() == 2 {
            return Ok(ParameterExpansion {
                variable: parts[0].to_string(),
                operator: ParameterExpansionOperator::DefaultValue(parts[1].to_string()),
                is_mutable: true,
            });
        }
    }
    if content.contains(":=") {
        let parts: Vec<&str> = content.splitn(2, ":=").collect();
        if parts.len() == 2 {
            return Ok(ParameterExpansion {
                variable: parts[0].to_string(),
                operator: ParameterExpansionOperator::AssignDefault(parts[1].to_string()),
                is_mutable: true,
            });
        }
    }
    if content.contains(":+") {
        let parts: Vec<&str> = content.splitn(2, ":+").collect();
        if parts.len() == 2 {
            return Ok(ParameterExpansion {
                variable: parts[0].to_string(),
                operator: ParameterExpansionOperator::DefaultValue(parts[1].to_string()),
                is_mutable: true,
            });
        }
    }
    if content.contains(":?") {
        let parts: Vec<&str> = content.splitn(2, ":?").collect();
        if parts.len() == 2 {
            return Ok(ParameterExpansion {
                variable: parts[0].to_string(),
                operator: ParameterExpansionOperator::ErrorIfUnset(parts[1].to_string()),
                is_mutable: true,
            });
        }
    }

    // Check for ${var:offset} or ${var:offset:length} - substring/array-slice
    // Single colon NOT followed by - = + ? : (those are handled above).
    // Must come AFTER the [...] check because patterns like ${arr[@]:offset} have
    // brackets and should be handled by the array-access branch first.
    // We only reach here for simple variable names (no brackets) like ${@:3} or ${var:offset}.

    // Check for operators that use `//` and `/` BEFORE checking for array access,
    // because patterns like ${var//[^a-z]/_} contain brackets that look like array access.
    // Must check these before the array-access branch.

    // Check for // pattern substitution: ${var//pattern/replacement}
    if content.contains("//") {
        let parts: Vec<&str> = content.splitn(2, "//").collect();
        if parts.len() == 2 {
            let pattern_replacement = parts[1];
            if let Some(slash_pos) = pattern_replacement.find('/') {
                let pattern = &pattern_replacement[..slash_pos];
                let replacement = &pattern_replacement[slash_pos + 1..];
                return Ok(ParameterExpansion {
                    variable: parts[0].to_string(),
                    operator: ParameterExpansionOperator::SubstituteAll(
                        pattern.to_string(),
                        replacement.to_string(),
                    ),
                    is_mutable: true,
                });
            }
        }
    }
    // Single / pattern substitution: ${var/pattern/replacement} (first
    // occurrence only — SubstituteFirst).
    if content.contains('/') {
        let parts: Vec<&str> = content.splitn(3, '/').collect();
        if parts.len() == 3 {
            return Ok(ParameterExpansion {
                variable: parts[0].to_string(),
                operator: ParameterExpansionOperator::SubstituteFirst(
                    parts[1].to_string(),
                    parts[2].to_string(),
                ),
                is_mutable: true,
            });
        }
    }

    // Check for array/map access: arr[1], map[foo]
    // IMPORTANT: This check must come AFTER the // and / substitution checks above,
    // because a regex character class like [a-z] inside a substitution pattern
    // would otherwise be misinterpreted as array access.
    if content.contains('[') && content.contains(']') {
        // Guard: if the part BEFORE the `[` contains pattern-removal or
        // substitution operators (`${x##*[/\\]}` — a bracket CLASS inside
        // the pattern, not a subscript), fall through to the operator
        // checks below (mirrors the parse_variable_expansion copy).
        let pattern_op_before_bracket = content.find('[').map_or(false, |bs| {
            content[..bs].contains('#') || content[..bs].contains('%') || content[..bs].contains('/')
        });
        if !pattern_op_before_bracket {
        if let Some(bracket_start) = content.find('[') {
            if let Some(bracket_end) = content.rfind(']') {
                let var_name = &content[..bracket_start];
                let key = &content[bracket_start + 1..bracket_end];

                // Special case: if key is "@", this is array iteration
                if key == "@" {
                    // Check if there is a slice specification after the bracket: @]:offset:length
                    let rest = &content[bracket_end + 1..];
                    if rest.starts_with(':') {
                        let slice_spec = &rest[1..]; // skip ':'
                        if let Some(colon_pos) = slice_spec.find(':') {
                            let offset = &slice_spec[..colon_pos];
                            let length = &slice_spec[colon_pos + 1..];
                            return Ok(ParameterExpansion {
                                variable: var_name.to_string(),
                                operator: ParameterExpansionOperator::ArraySlice(
                                    offset.to_string(),
                                    Some(length.to_string()),
                                ),
                                is_mutable: true,
                            });
                        } else {
                            // offset without length
                            return Ok(ParameterExpansion {
                                variable: var_name.to_string(),
                                operator: ParameterExpansionOperator::ArraySlice(
                                    slice_spec.to_string(),
                                    None,
                                ),
                                is_mutable: true,
                            });
                        }
                    }
                    return Ok(ParameterExpansion {
                        variable: var_name.to_string(),
                        operator: ParameterExpansionOperator::ArraySlice("@".to_string(), None),
                        is_mutable: true,
                    });
                }

                // Check if there is an operator after the bracket
                let rest = &content[bracket_end + 1..];

                // Handle operators on array elements (rest contains the operator + pattern)
                if rest.starts_with("##") {
                    let pattern = &rest[2..];
                    return Ok(ParameterExpansion {
                        variable: format!("{}[{}]", var_name, key),
                        operator: ParameterExpansionOperator::RemoveLongestPrefix(
                            pattern.to_string(),
                        ),
                        is_mutable: true,
                    });
                } else if rest.starts_with('#') {
                    let pattern = &rest[1..];
                    return Ok(ParameterExpansion {
                        variable: format!("{}[{}]", var_name, key),
                        operator: ParameterExpansionOperator::RemoveShortestPrefix(
                            pattern.to_string(),
                        ),
                        is_mutable: true,
                    });
                } else if rest.starts_with("%%") {
                    let pattern = &rest[2..];
                    return Ok(ParameterExpansion {
                        variable: format!("{}[{}]", var_name, key),
                        operator: ParameterExpansionOperator::RemoveLongestSuffix(
                            pattern.to_string(),
                        ),
                        is_mutable: true,
                    });
                } else if rest.starts_with('%') {
                    let pattern = &rest[1..];
                    return Ok(ParameterExpansion {
                        variable: format!("{}[{}]", var_name, key),
                        operator: ParameterExpansionOperator::RemoveShortestSuffix(
                            pattern.to_string(),
                        ),
                        is_mutable: true,
                    });
                } else if rest == "^^" {
                    return Ok(ParameterExpansion {
                        variable: format!("{}[{}]", var_name, key),
                        operator: ParameterExpansionOperator::UppercaseAll,
                        is_mutable: true,
                    });
                } else if rest == ",," {
                    return Ok(ParameterExpansion {
                        variable: format!("{}[{}]", var_name, key),
                        operator: ParameterExpansionOperator::LowercaseAll,
                        is_mutable: true,
                    });
                } else if rest == "^" {
                    return Ok(ParameterExpansion {
                        variable: format!("{}[{}]", var_name, key),
                        operator: ParameterExpansionOperator::UppercaseFirst,
                        is_mutable: true,
                    });
                } else if rest.starts_with('/') {
                    // This is ${arr[1]/pattern/replacement} - substitution on an array element
                    // We'll handle this later - for now, just treat as array access
                } else if !rest.is_empty() && !rest.starts_with(':') {
                    // Trailing junk after `]` (e.g. `${arr[1]>2}`, `${arr[1]foo}`):
                    // bash rejects the whole expansion as a "bad substitution"
                    // (skips the command, status 1). A `:` continuation is a
                    // valid element slice — falls through below (pre-existing
                    // behavior).
                    return Ok(ParameterExpansion {
                        variable: content.to_string(),
                        operator: ParameterExpansionOperator::BadSubstitution,
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
    }

    // Check for ${var:offset} or ${var:offset:length} - substring/array-slice
    // Single colon NOT followed by - = + ? : (those are handled above).
    // Must come AFTER the [...] check because patterns like ${arr[@]:offset} have
    // brackets and should be handled by the array-access branch first.
    // We only reach here for simple variable names (no brackets) like ${@:3} or ${var:offset}.
    // Also guard against operator patterns like ${var%%pattern} where the pattern
    // contains ':' by checking that no operator characters precede the colon.
    if content.contains(':')
        && !content.contains('[')
        && !content.contains(']')
        && !content.contains("::")
        && !content.contains(":-")
        && !content.contains(":=")
        && !content.contains(":+")
        && !content.contains(":?")
        && !content.contains('%')
        && !content.contains('#')
        && !content.contains('/')
        && !content.contains('^')
        && !content.contains(',')
    {
        let colon_pos = content.find(':').unwrap();
        // Only treat as ArraySlice if the colon is at a position that could be
        // after a variable name (not after an operator).
        if colon_pos > 0 {
            let var_name = &content[..colon_pos];
            let rest = &content[colon_pos + 1..];
            if let Some(second_colon) = rest.find(':') {
                let offset = &rest[..second_colon];
                let length = &rest[second_colon + 1..];
                return Ok(ParameterExpansion {
                    variable: var_name.to_string(),
                    operator: ParameterExpansionOperator::ArraySlice(
                        offset.to_string(),
                        Some(length.to_string()),
                    ),
                    is_mutable: true,
                });
            } else {
                return Ok(ParameterExpansion {
                    variable: var_name.to_string(),
                    operator: ParameterExpansionOperator::ArraySlice(rest.to_string(), None),
                    is_mutable: true,
                });
            }
        }
    }

    // Check for parameter expansion operators (except // and / which are checked above)
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
    } else if content.ends_with("%/*") && !content.ends_with("%%/*") {
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
    } else if content.contains("%%") && !(content.ends_with("%/*") && !content.ends_with("%%/*")) {
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
    } else if content.contains("%")
        && !content.contains("%%")
        && !(content.ends_with("%/*") && !content.ends_with("%%/*"))
    {
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
    } else if content.contains(":-") {
        let parts: Vec<&str> = content.splitn(2, ":-").collect();
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
        let parts: Vec<&str> = content.splitn(2, ":=").collect();
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
        let parts: Vec<&str> = content.splitn(2, ":?").collect();
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
    } else if content.contains('-')
        && !content.contains('%')
        && !content.contains('#')
        && !content.contains('/')
        && !content.contains('!')
        && !content.contains(':')
    {
        // ${var-default} - use default if var is unset (not if empty)
        let parts: Vec<&str> = content.splitn(2, '-').collect();
        if parts.len() == 2 && !parts[0].is_empty() {
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
    } else if content.contains("=-") {
        // ${var=-default} - assign default if var is unset
        let parts: Vec<&str> = content.splitn(2, "=-").collect();
        if parts.len() == 2
            && !parts[0].is_empty()
            && !parts[0].contains('%')
            && !parts[0].contains('#')
        {
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
    } else if let Some(quest_pos) = content.find('?') {
        // ${var?error} - error if var is UNSET (the `?` without a colon;
        // `:?` is handled above). Must come AFTER the -/=- branches so
        // `${var-?}` / `${var=-?}` stay default-value forms. Patterns with
        // `?` (${var%?}, ${var#?}, ${var/pat?/rep}) are handled by their
        // own branches above.
        if quest_pos > 0 {
            let var_name = &content[..quest_pos];
            let error_msg = &content[quest_pos + 1..];
            Ok(ParameterExpansion {
                variable: var_name.to_string(),
                operator: ParameterExpansionOperator::ErrorIfUnset(error_msg.to_string()),
                is_mutable: true,
            })
        } else {
            // `${?}` — the `$?` special var in braces
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
        return Err(ParserError::InvalidSyntax(
            "Invalid ANSI-C quoted string format".to_string(),
        ));
    }

    let content = &raw_text[2..raw_text.len() - 1]; // Remove $' and '

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
                    '"' => result.push('"'),    // Double quote
                    '?' => result.push('?'),    // Question mark
                    '0' => result.push('\0'),   // Null byte
                    'x' => {
                        // Hex escape: \xHH
                        let mut hex_chars = String::new();
                        for _ in 0..2 {
                            if let Some(hex_ch) = chars.next() {
                                if hex_ch.is_ascii_hexdigit() {
                                    hex_chars.push(hex_ch);
                                } else {
                                    return Err(ParserError::InvalidSyntax(format!(
                                        "Invalid hex escape: \\x{}",
                                        hex_ch
                                    )));
                                }
                            } else {
                                return Err(ParserError::InvalidSyntax(
                                    "Incomplete hex escape".to_string(),
                                ));
                            }
                        }
                        if let Ok(byte_val) = u8::from_str_radix(&hex_chars, 16) {
                            result.push(byte_val as char);
                        } else {
                            return Err(ParserError::InvalidSyntax(format!(
                                "Invalid hex value: {}",
                                hex_chars
                            )));
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
                                    return Err(ParserError::InvalidSyntax(format!(
                                        "Invalid unicode escape: \\u{}",
                                        hex_ch
                                    )));
                                }
                            } else {
                                return Err(ParserError::InvalidSyntax(
                                    "Incomplete unicode escape".to_string(),
                                ));
                            }
                        }
                        if let Ok(unicode_val) = u32::from_str_radix(&hex_chars, 16) {
                            if let Some(unicode_char) = char::from_u32(unicode_val) {
                                result.push(unicode_char);
                            } else {
                                return Err(ParserError::InvalidSyntax(format!(
                                    "Invalid unicode value: {}",
                                    unicode_val
                                )));
                            }
                        } else {
                            return Err(ParserError::InvalidSyntax(format!(
                                "Invalid unicode hex value: {}",
                                hex_chars
                            )));
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
                                    return Err(ParserError::InvalidSyntax(format!(
                                        "Invalid extended unicode escape: \\U{}",
                                        hex_ch
                                    )));
                                }
                            } else {
                                return Err(ParserError::InvalidSyntax(
                                    "Incomplete extended unicode escape".to_string(),
                                ));
                            }
                        }
                        if let Ok(unicode_val) = u32::from_str_radix(&hex_chars, 16) {
                            if let Some(unicode_char) = char::from_u32(unicode_val) {
                                result.push(unicode_char);
                            } else {
                                return Err(ParserError::InvalidSyntax(format!(
                                    "Invalid extended unicode value: {}",
                                    unicode_val
                                )));
                            }
                        } else {
                            return Err(ParserError::InvalidSyntax(format!(
                                "Invalid extended unicode hex value: {}",
                                hex_chars
                            )));
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

    Ok(Word::Literal(result, None))
}

fn parse_brace_expansion(lexer: &mut Lexer) -> Result<Word, ParserError> {
    use crate::ast::{BraceExpansion, BraceItem, BraceRange};

    // Consume the opening brace
    if !matches!(lexer.peek(), Some(Token::BraceOpen)) {
        return Err(ParserError::InvalidSyntax(
            "Expected '{' for brace expansion".to_string(),
        ));
    }
    lexer.next(); // consume '{'

    let mut items: Vec<BraceItem> = Vec::new();

    // Accumulator for consecutive literal text between commas.
    // In a brace expansion like {file.txt,file.bak}, the items between commas
    // can consist of multiple tokens (Identifier, Dot, Identifier, etc.) that
    // should be merged into a single BraceItem::Literal.
    let mut acc: Option<String> = None;

    /// Helper: flush the accumulator as a BraceItem::Literal.
    fn flush_acc(items: &mut Vec<BraceItem>, acc: &mut Option<String>) {
        if let Some(text) = acc.take() {
            if !text.is_empty() {
                items.push(BraceItem::Literal(text));
            }
        }
    }

    // Parse the content inside braces
    loop {
        match lexer.peek() {
            Some(Token::BraceClose) => {
                flush_acc(&mut items, &mut acc);
                lexer.next(); // consume '}'
                break;
            }
            Some(Token::Number) | Some(Token::Float) | Some(Token::PaddedNumber) => {
                let start = lexer.get_number_text()?;

                // Check if this is a range (look for ..)
                if matches!(lexer.peek(), Some(Token::Range)) {
                    flush_acc(&mut items, &mut acc);
                    lexer.next(); // consume '..'

                    if let Some(Token::Number) | Some(Token::PaddedNumber) = lexer.peek() {
                        let end = lexer.get_number_text()?;

                        // Check if there's a step value (another ..)
                        if matches!(lexer.peek(), Some(Token::Range)) {
                            lexer.next(); // consume second '..'

                            if let Some(Token::Number) | Some(Token::PaddedNumber) = lexer.peek() {
                                let step = lexer.get_number_text()?;
                                items.push(BraceItem::Range(BraceRange {
                                    start,
                                    end,
                                    step: Some(step),
                                    format: None,
                                }));
                                continue;
                            } else {
                                return Err(ParserError::InvalidSyntax(
                                    "Expected number after second '..' in brace range".to_string(),
                                ));
                            }
                        } else {
                            items.push(BraceItem::Range(BraceRange {
                                start,
                                end,
                                step: None,
                                format: None,
                            }));
                            continue;
                        }
                    } else {
                        return Err(ParserError::InvalidSyntax(
                            "Expected number after '..' in brace range".to_string(),
                        ));
                    }
                } else {
                    // Literal number — accumulate into current text
                    acc.get_or_insert_with(String::new).push_str(&start);
                }
            }
            Some(Token::Identifier) => {
                let text = lexer.get_identifier_text()?;

                // Check if this is a range (look for ..)
                if matches!(lexer.peek(), Some(Token::Range)) {
                    flush_acc(&mut items, &mut acc);
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
                                continue;
                            } else {
                                return Err(ParserError::InvalidSyntax(
                                    "Expected number after second '..' in identifier brace range"
                                        .to_string(),
                                ));
                            }
                        } else {
                            items.push(BraceItem::Range(BraceRange {
                                start: text,
                                end,
                                step: None,
                                format: None,
                            }));
                            continue;
                        }
                    } else {
                        return Err(ParserError::InvalidSyntax(
                            "Expected identifier after '..' in brace range".to_string(),
                        ));
                    }
                } else {
                    // Literal identifier — accumulate into current text
                    acc.get_or_insert_with(String::new).push_str(&text);
                }
            }
            Some(Token::BraceOpen) => {
                flush_acc(&mut items, &mut acc);
                let nested = parse_brace_expansion(lexer)?;
                if let Word::BraceExpansion(be, _) = nested {
                    items.push(BraceItem::Nested(Box::new(be)));
                } else {
                    return Err(ParserError::InvalidSyntax(
                        "Expected brace expansion from nested brace".to_string(),
                    ));
                }
            }
            Some(Token::Comma) => {
                // Comma terminates the current accumulated item.
                flush_acc(&mut items, &mut acc);
                lexer.next(); // consume ','
            }
            Some(
                Token::Slash
                | Token::Dot
                | Token::Colon
                | Token::Minus
                | Token::Plus
                | Token::Star
                | Token::At
                | Token::Bang
                | Token::Percent
                | Token::Caret
                | Token::Tilde
                | Token::Escape
                | Token::EscapedDoubleQuote
                | Token::EscapedSingleQuote
                | Token::HexNumber
                | Token::Semicolon
                | Token::NonZero
                | Token::Size
                | Token::File
                | Token::Directory
                | Token::Readable
                | Token::PipeFile
                | Token::TestBracket
                | Token::TestBracketClose,
            ) => {
                let text = lexer.get_current_text().unwrap_or_default();
                lexer.next();
                // Accumulate into current text instead of creating a separate item
                acc.get_or_insert_with(String::new).push_str(&text);
            }
            None => {
                flush_acc(&mut items, &mut acc);
                break;
            }
            _ => {
                // Instead of erroring, treat unexpected tokens as literal text.
                if let Some(text) = lexer.get_current_text() {
                    acc.get_or_insert_with(String::new).push_str(&text);
                    lexer.next();
                } else {
                    flush_acc(&mut items, &mut acc);
                    break;
                }
            }
        }
    }

    // Flush any remaining accumulated text
    flush_acc(&mut items, &mut acc);

    Ok(Word::BraceExpansion(
        BraceExpansion {
            prefix: None,
            items,
            suffix: None,
        },
        None,
    ))
}

fn parse_arithmetic_expression(lexer: &mut Lexer) -> Result<Word, ParserError> {
    // Parse arithmetic expressions like $((i + 1))
    // First, consume the opening $(( or $(
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

    // Capture the content until we find the closing ))
    // Track actual parenthesis depth: $(( or (( contributes 2 opening parens,
    // and each ArithmeticEvalClose ("))") contributes 2 closing parens.
    // Regular ParenOpen/ParenClose tokens adjust depth by 1.
    let mut expression_parts = Vec::new();
    let mut paren_depth = 2; // $(( or (( contributes 2 opening parens

    loop {
        match lexer.peek() {
            Some(Token::ArithmeticEvalClose) => {
                // This is the closing )) for $((...)) or ((...))
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
                // Regular opening parenthesis inside the expression
                if let Some(text) = lexer.get_current_text() {
                    expression_parts.push(text);
                }
                lexer.next();
                paren_depth += 1;
            }
            Some(Token::ParenClose) => {
                // Regular closing parenthesis inside the expression
                paren_depth -= 1;
                // Only push `)` if it closes an inner (expression) paren,
                // not if it closes the outer $(( or (( marker.
                // Inner parens keep depth >= 2 (the 2 from $((/本身就是 outer).
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
                // This is another opening $((...)) or nested ((...))
                lexer.next();
                paren_depth += 2;
                expression_parts.push("$((".to_string());
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
            Some(Token::Percent) => {
                expression_parts.push("%".to_string());
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
            Some(Token::DollarParen) => {
                // Nested $(...) command substitution inside arithmetic
                paren_depth += 1;
                if let Some(text) = lexer.get_current_text() {
                    expression_parts.push(text);
                }
                lexer.next();
            }
            Some(Token::Dollar) => {
                // Handle variable references like $i, $1, $2, etc.
                lexer.next();
                match lexer.peek() {
                    Some(Token::Identifier) => {
                        let var_name = lexer.get_identifier_text()?;
                        expression_parts.push(format!("${}", var_name));
                    }
                    Some(Token::Number) => {
                        let num = lexer.get_number_text()?;
                        expression_parts.push(format!("${}", num));
                    }
                    _ => {
                        // For special $ vars ($?, $$, $!, $-, $#, $@, $*)
                        // these are already separate tokens (DollarQuestion,
                        // DollarDollar, etc.) and fall through to the catch-all
                        // below. But if we see a bare $ followed by something
                        // unexpected, report an error.
                        return Err(ParserError::InvalidSyntax(
                            "Expected identifier after $ in arithmetic expression".to_string(),
                        ));
                    }
                }
            }
            None => {
                return Err(ParserError::InvalidSyntax(
                    "Unexpected end of input in arithmetic expression".to_string(),
                ));
            }
            Some(Token::Comment) => {
                // A `#` inside an arithmetic expression is the base-notation operator
                // (e.g. 10#$x), not a comment start.  The logos lexer, however, tokenises
                // `#...` as a Comment.  Use scan_arithmetic_comment to extract the content
                // before the closing `))` and inject the `))` and any following text
                // (e.g. `; then`) as new tokens.  The normal ArithmeticEvalClose case
                // below will handle the `))`.
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
    Ok(Word::Arithmetic(
        ArithmeticExpression {
            expression,
            tokens: Vec::new(), // We don't need to store individual tokens for now
        },
        None,
    ))
}

fn parse_arithmetic_bracket(lexer: &mut Lexer) -> Result<Word, ParserError> {
    // Parse $[...] arithmetic expressions (deprecated but supported in bash)
    // Consume the opening $[
    lexer.next(); // consume $[

    let mut expression_parts = Vec::new();
    let mut bracket_depth = 1; // we just consumed $[

    loop {
        match lexer.peek() {
            Some(Token::TestBracket) => {
                // Opening [ inside expression (nested array subscript?)
                bracket_depth += 1;
                expression_parts.push("[".to_string());
                lexer.next();
            }
            Some(Token::TestBracketClose) => {
                lexer.next();
                bracket_depth -= 1;
                if bracket_depth <= 0 {
                    break;
                }
                expression_parts.push("]".to_string());
            }
            Some(Token::DollarBrace) => {
                // ${...} variable expansion inside arithmetic
                if let Some(text) = lexer.get_current_text() {
                    expression_parts.push(text);
                }
                lexer.next();
                // Parse until the matching }
                loop {
                    match lexer.peek() {
                        Some(Token::BraceClose) => {
                            expression_parts.push("}".to_string());
                            lexer.next();
                            break;
                        }
                        Some(_) => {
                            if let Some(text) = lexer.get_current_text() {
                                expression_parts.push(text);
                            }
                            lexer.next();
                        }
                        None => {
                            return Err(ParserError::InvalidSyntax(
                                "Unclosed ${} inside arithmetic bracket".to_string(),
                            ));
                        }
                    }
                }
            }
            None => {
                return Err(ParserError::InvalidSyntax(
                    "Unexpected end of input in arithmetic bracket expression".to_string(),
                ));
            }
            _ => {
                // Any other token: consume and add its text
                if let Some(text) = lexer.get_current_text() {
                    expression_parts.push(text);
                }
                lexer.next();
            }
        }
    }

    let expression = expression_parts.join("");

    Ok(Word::Arithmetic(
        ArithmeticExpression {
            expression,
            tokens: Vec::new(),
        },
        None,
    ))
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
                Some(
                    Token::DollarBrace
                    | Token::DollarBraceHash
                    | Token::DollarBraceBang
                    | Token::DollarBraceStar
                    | Token::DollarBraceAt
                    | Token::DollarBraceHashStar
                    | Token::DollarBraceHashAt
                    | Token::DollarBraceBangStar
                    | Token::DollarBraceBangAt,
                ) => {
                    // Nested ${...} — increment depth so the matching } doesn't
                    // close our outer brace prematurely.
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
                Some(Token::Comment) => {
                    // A `#` inside ${...} is a parameter expansion operator (${var#pattern},
                    // ${var##pattern}), not a comment start.  The logos lexer, however,
                    // tokenises `#...` as a Comment.
                    //
                    // Use the lexer's handle_comment_with_brace which finds the first `}`
                    // inside the comment text, returns everything before it, and re-lexes
                    // any text after `}` so that subsequent tokens (e.g. `in`) are not lost.
                    let text = lexer.get_text(start, end);
                    if text.contains('}') {
                        let before = lexer.handle_comment_with_brace(brace_depth)?;
                        content.push_str(&before);
                        brace_depth -= 1;
                        if brace_depth == 0 {
                            break;
                        }
                        // handle_comment_with_brace removed the Comment token and injected
                        // re-lexed tokens.  Do NOT call lexer.next() — current already
                        // points at the first injected token.
                    } else {
                        // No `}` means this # is just part of a variable name or pattern
                        // (e.g. ${#var} for length).  Consume the comment as literal text.
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
    Err(ParserError::InvalidSyntax(
        "Parameter expansion not yet implemented".to_string(),
    ))
}

fn parse_array_slicing(_lexer: &mut Lexer, _array_name: String) -> Result<Word, ParserError> {
    // TODO: Implement array slicing parsing
    Err(ParserError::InvalidSyntax(
        "Array slicing not yet implemented".to_string(),
    ))
}

fn parse_backtick_command_substitution(lexer: &mut Lexer) -> Result<Word, ParserError> {
    // Parse backtick command substitution
    let backtick_text = lexer.get_raw_token_text()?;
    // Remove the surrounding backticks
    let command_text = &backtick_text[1..backtick_text.len() - 1];

    // Check if the command contains command substitutions (like $(pwd)), pipelines (like |), or logical operators (like && or ||)
    if command_text.contains("$(")
        || command_text.contains("|")
        || command_text.contains("&&")
        || command_text.contains("||")
    {
        // Use the full parser for commands with command substitutions, pipelines, or logical operators
        let sub_lexer = Lexer::new(command_text);
        let mut sub_parser = Parser::new_with_lexer(sub_lexer);
        match sub_parser.parse() {
            Ok(commands) => {
                if commands.len() == 1 {
                    Ok(Word::CommandSubstitution(
                        Box::new(commands[0].clone()),
                        None,
                    ))
                } else if commands.is_empty() {
                    // If no commands parsed, treat as a simple command with the text as argument
                    let placeholder_cmd = Command::Simple(SimpleCommand {
                        name: Word::Literal("echo".to_string(), None),
                        args: vec![Word::Literal(command_text.to_string(), None)],
                        redirects: Vec::new(),
                        env_vars: BTreeMap::new(),
                        stdout_used: true,
                        stderr_used: true,
                    });
                    Ok(Word::CommandSubstitution(Box::new(placeholder_cmd), None))
                } else {
                    // If multiple commands, use the first one
                    Ok(Word::CommandSubstitution(
                        Box::new(commands[0].clone()),
                        None,
                    ))
                }
            }
            Err(_) => {
                // Fall back to using the simple command parser
                match crate::parser::commands::parse_pipeline_from_text(command_text) {
                    Ok(command) => Ok(Word::CommandSubstitution(Box::new(command), None)),
                    Err(_) => {
                        // Fall back to treating it as a literal
                        Ok(Word::Literal(format!("`{}`", command_text), None))
                    }
                }
            }
        }
    } else {
        // Use the simple command parser for commands without command substitutions
        match crate::parser::commands::parse_pipeline_from_text(command_text) {
            Ok(command) => Ok(Word::CommandSubstitution(Box::new(command), None)),
            Err(_) => {
                // Fall back to treating it as a literal
                Ok(Word::Literal(format!("`{}`", command_text), None))
            }
        }
    }
}

// Helper function to parse a simple command from text
fn parse_simple_command_from_text(text: &str) -> Result<Command, ParserError> {
    use crate::lexer::{Lexer, Token};

    // Create a lexer for the command text
    let mut lexer = Lexer::new(text);
    let mut args = Vec::new();
    let mut redirects = Vec::new();
    let mut current_arg = String::new();
    let mut in_quotes = false;
    let mut quote_char = '\0';

    // Process tokens and group them into arguments or redirections
    while let Some(token) = lexer.peek() {
        match token {
            Token::Space => {
                lexer.next(); // consume the space
                if in_quotes {
                    current_arg.push(' ');
                } else if !current_arg.is_empty() {
                    args.push(current_arg.clone());
                    current_arg.clear();
                }
            }
            Token::Comment => break, // Stop at comments
            Token::RedirectIn
            | Token::RedirectOut
            | Token::RedirectAppend
            | Token::RedirectInErr
            | Token::RedirectOutErr
            | Token::RedirectInOut
            | Token::Heredoc
            | Token::HeredocTabs
            | Token::HereString => {
                // This is a redirection operator
                // First, add any current argument
                if !current_arg.is_empty() {
                    args.push(current_arg.clone());
                    current_arg.clear();
                }

                // Parse the redirection (don't consume the token here, let parse_redirect handle it)
                let redirect = parse_redirect(&mut lexer)?;
                redirects.push(redirect);
            }
            Token::Number => {
                // Check if this is a file descriptor redirection (number followed by redirect operator)
                if let Some(next_token) = lexer.peek_n(1) {
                    match next_token {
                        Token::RedirectIn
                        | Token::RedirectOut
                        | Token::RedirectAppend
                        | Token::RedirectInErr
                        | Token::RedirectOutErr
                        | Token::RedirectInOut
                        | Token::Heredoc
                        | Token::HeredocTabs
                        | Token::HereString => {
                            // This is a file descriptor redirection
                            // First, add any current argument
                            if !current_arg.is_empty() {
                                args.push(current_arg.clone());
                                current_arg.clear();
                            }

                            // Parse the redirection (don't consume the number token here, let parse_redirect handle it)
                            let redirect = parse_redirect(&mut lexer)?;
                            redirects.push(redirect);
                        }
                        _ => {
                            // This is just a regular number argument
                            lexer.next(); // consume the number
                            if let Some((_, start, end)) = lexer.tokens.get(lexer.current - 1) {
                                let token_text = &text[*start..*end];
                                current_arg.push_str(token_text);
                            }
                        }
                    }
                } else {
                    // No next token, treat as regular number argument
                    lexer.next(); // consume the number
                    if let Some((_, start, end)) = lexer.tokens.get(lexer.current - 1) {
                        let token_text = &text[*start..*end];
                        current_arg.push_str(token_text);
                    }
                }
            }
            Token::Identifier => {
                // Handle identifier as literal
                lexer.next();
                if let Some((_, start, end)) = lexer.tokens.get(lexer.current - 1) {
                    let token_text = &text[*start..*end];
                    current_arg.push_str(token_text);
                }
            }
            Token::Plus => {
                // Handle plus character as literal
                lexer.next();
                if let Some((_, start, end)) = lexer.tokens.get(lexer.current - 1) {
                    let token_text = &text[*start..*end];
                    current_arg.push_str(token_text);
                }
            }
            Token::Minus => {
                // Handle minus character as literal
                lexer.next();
                if let Some((_, start, end)) = lexer.tokens.get(lexer.current - 1) {
                    let token_text = &text[*start..*end];
                    current_arg.push_str(token_text);
                }
            }
            Token::Percent => {
                // Handle percent character as literal
                lexer.next();
                if let Some((_, start, end)) = lexer.tokens.get(lexer.current - 1) {
                    let token_text = &text[*start..*end];
                    current_arg.push_str(token_text);
                }
            }
            Token::DollarParen => {
                // Handle $(...) command substitution
                let command_text = lexer.capture_parenthetical_text()?;
                // For now, just add as literal - this is a limitation of parse_simple_command_from_text
                // The proper fix would require changing the return type to handle command substitutions
                args.push(format!("$({})", command_text));
            }
            _ => {
                // Consume the token
                lexer.next();
                // Get the token text from the current position
                if let Some((_, start, end)) = lexer.tokens.get(lexer.current - 1) {
                    let token_text = &text[*start..*end];

                    // Handle quoted strings
                    if (token_text.starts_with('"') || token_text.starts_with('\'')) && !in_quotes {
                        quote_char = token_text.chars().next().unwrap();
                        if token_text.ends_with(quote_char) && token_text.len() > 1 {
                            // Complete quoted string in one token
                            current_arg.push_str(&token_text[1..token_text.len() - 1]);
                        } else {
                            // Start of quoted string
                            in_quotes = true;
                            current_arg.push_str(&token_text[1..]);
                        }
                    } else if in_quotes && token_text.ends_with(quote_char) {
                        // End of quoted string
                        current_arg.push_str(&token_text[..token_text.len() - 1]);
                        in_quotes = false;
                    } else {
                        // Regular token - add to current argument
                        current_arg.push_str(token_text);
                    }
                }
            }
        }
    }

    // Add the last argument if it exists
    if !current_arg.is_empty() {
        args.push(current_arg);
    }

    if args.is_empty() {
        return Err(ParserError::InvalidSyntax(
            "Empty command in backticks".to_string(),
        ));
    }

    // First argument is the command name
    let name = Word::Literal(args[0].clone(), None);

    // Remaining arguments
    let mut word_args = Vec::new();
    for arg in &args[1..] {
        word_args.push(Word::Literal(arg.clone(), None));
    }

    let cmd = Command::Simple(SimpleCommand {
        name,
        args: word_args,
        redirects,
        env_vars: BTreeMap::new(),
        stdout_used: true,
        stderr_used: true,
    });

    Ok(cmd)
}
