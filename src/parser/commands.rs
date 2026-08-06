use crate::ast::*;
use crate::lexer::{Lexer, Token};
use crate::parser::assignments::parse_array_elements;
use crate::parser::control_flow::{
    parse_block, parse_break_statement, parse_case_statement, parse_continue_statement,
    parse_for_loop, parse_function, parse_if_statement, parse_posix_function,
    parse_return_statement, parse_until_loop, parse_while_loop,
};
use crate::parser::errors::ParserError;
use crate::parser::redirects::parse_redirect;
use crate::parser::utilities::ParserUtilities;
use crate::parser::words::{parse_word, parse_word_no_newline_skip};
use std::collections::{BTreeMap, HashMap};

/// Convert a Word into a Vec of StringParts for use in StringInterpolation.
fn word_to_parts(word: Word) -> Vec<StringPart> {
    match word {
        Word::Literal(s, _) => vec![StringPart::Literal(s)],
        Word::StringInterpolation(interp, _) => interp.parts,
        Word::Variable(v, _, _) => vec![StringPart::Variable(v)],
        Word::CommandSubstitution(cmd, _) => vec![StringPart::CommandSubstitution(cmd)],
        Word::ParameterExpansion(pe, _) => vec![StringPart::ParameterExpansion(pe)],
        Word::Arithmetic(ae, _) => vec![StringPart::Arithmetic(ae)],
        other => vec![StringPart::Literal(other.to_string())],
    }
}

pub struct Parser {
    pub lexer: Lexer,
    shopt_state: TestModifiers,
}

impl Parser {
    pub fn new(input: &str) -> Self {
        Self {
            lexer: Lexer::new(input),
            shopt_state: TestModifiers::default(),
        }
    }

    pub fn new_with_lexer(lexer: Lexer) -> Self {
        Self {
            lexer,
            shopt_state: TestModifiers::default(),
        }
    }

    pub fn parse(&mut self) -> Result<Vec<Command>, ParserError> {
        let mut commands = vec![];

        // Skip initial whitespace but preserve newlines for proper command separation
        let mut newline_count = 0;
        loop {
            match self.lexer.peek() {
                Some(Token::Space) | Some(Token::Tab) | Some(Token::Comment) => {
                    self.lexer.next();
                }
                Some(Token::Newline) | Some(Token::CarriageReturn) => {
                    newline_count += 1;
                    self.lexer.next();
                }
                _ => break,
            }
        }

        // Index of the first command parsed on the CURRENT source line. bash
        // parses/executes one line (list) at a time; a syntax error (`;;` or a
        // stray `)` outside case/subshell) discards the whole line and aborts
        // the rest of the script, so the parser truncates back to this index.
        let mut line_start = 0usize;

        while !self.lexer.is_eof() {

            let _current_token = self.lexer.peek();

            if self.lexer.is_eof() {
                break;
            }

            // Check if we're at a newline before parsing the command
            if let Some(Token::Newline) | Some(Token::CarriageReturn) = self.lexer.peek() {
                // Consume the token and continue to next iteration
                line_start = commands.len();
                self.lexer.next();
                continue;
            }

            let mut command = self.parse_command()?;

            if let Command::Simple(ref simple_cmd) = command {
                if simple_cmd.name.as_literal().unwrap_or("") == "" && simple_cmd.args.is_empty() {
                    // This is an empty command from a newline, skip it
                    continue;
                }
            }

            // After parsing a command, look ahead for pipeline operators
            // Skip whitespace and comments (tracking line boundaries: a
            // newline here starts a new source line, so `;;`-truncation must
            // keep the commands before it)
            loop {
                match self.lexer.peek() {
                    Some(Token::Space) | Some(Token::Tab) | Some(Token::Comment) => {
                        self.lexer.next();
                    }
                    Some(Token::Newline) | Some(Token::CarriageReturn) => {
                        // The command parsed above is pushed AFTER this loop,
                        // so it starts the new line too.
                        line_start = commands.len() + 1;
                        self.lexer.next();
                    }
                    _ => break,
                }
            }

            // Check if the next token is a pipeline operator
            if let Some(token) = self.lexer.peek() {
                match token {
                    Token::And | Token::Or | Token::Pipe => {
                        // This command is part of a pipeline, parse the rest
                        // For pipeline continuation, we don't need to capture source text again
                        let dummy_start = 0;
                        command = self.parse_pipeline_from_command(command, dummy_start)?;
                    }
                    _ => {}
                }
            }

            commands.push(command);

            // Handle separators and comments after command
            newline_count = 0;
            loop {
                match self.lexer.peek() {
                    Some(Token::Space) | Some(Token::Tab) | Some(Token::Comment) => {
                        self.lexer.next();
                    }
                    Some(Token::Newline) | Some(Token::CarriageReturn) => {
                        newline_count += 1;
                        line_start = commands.len();
                        self.lexer.next();
                    }
                    Some(Token::Semicolon) => {
                        self.lexer.next();
                        break;
                    }
                    Some(Token::DoubleSemicolon) => {
                        // `;;` outside a case body is a syntax error in
                        // bash (the whole script aborts, exit 2) — a real
                        // parse failure, not a recoverable one. The old
                        // truncate-and-succeed recovery silently DROPPED the
                        // rest of the script and exited 0 (parse-double-
                        // semicolon.sh, parse-error-doublesemicolon.sh:
                        // bash=2 vs estree=0). The CLI's parse-error
                        // fallback now reproduces bash's verdict.
                        return Err(ParserError::InvalidSyntax(
                            "`;;` outside case context".to_string(),
                        ));
                    }
                    Some(Token::ParenClose) => {
                        // A stray `)` after a command (outside any
                        // subshell — the subshell body loop pre-checks
                        // ParenClose): bash runs the commands BEFORE it,
                        // then aborts with a syntax error (exit 2). Recover
                        // the `)` as a literal command — the ESTree
                        // runner's stray-`)` path (exit 2) then matches
                        // bash; the Perl renderer's handling is
                        // best-effort.
                        self.lexer.next();
                        commands.push(Command::Simple(SimpleCommand {
                            name: Word::literal(")".to_string()),
                            args: vec![],
                            redirects: vec![],
                            env_vars: BTreeMap::new(),
                            stdout_used: true,
                            stderr_used: true,
                        }));
                        return Ok(commands);
                    }
                    Some(Token::Background) => {
                        // Convert last command to background
                        if let Some(last_command) = commands.pop() {
                            commands.push(Command::Background(Box::new(last_command)));
                        }
                        self.lexer.next();
                        // Skip whitespace and comments after & but preserve newlines
                        loop {
                            match self.lexer.peek() {
                                Some(Token::Space) | Some(Token::Tab) | Some(Token::Comment) => {
                                    self.lexer.next();
                                }
                                _ => break,
                            }
                        }
                        break;
                    }
                    _ => {
                        break;
                    }
                }
            }

            if newline_count >= 2 {
                commands.push(Command::BlankLine);
            }
        }

        Ok(commands)
    }

    /// Starting from offset `start`, return the offset of the first non-whitespace token.
    fn skip_ws_offset(&self, start: usize) -> usize {
        let mut pos = start;
        while matches!(
            self.lexer.peek_n(pos),
            Some(Token::Space | Token::Tab | Token::Comment | Token::Newline)
        ) {
            pos += 1;
        }
        pos
    }

    pub fn parse_command(&mut self) -> Result<Command, ParserError> {

        // Skip whitespace and comments, but NOT newlines
        // Newlines need to be handled as command separators
        while let Some(token) = self.lexer.peek() {
            match token {
                Token::Space | Token::Tab | Token::Comment => {
                    self.lexer.next();
                }
                _ => break,
            }
        }

        if self.lexer.is_eof() {
            return Err(ParserError::UnexpectedEOF);
        }

        let command = if let Some(Token::Identifier) = self.lexer.peek() {
            // Check if this is a function definition: identifier() { ... }
            // Allow whitespace between identifier and parentheses
            let paren_idx = self.skip_ws_offset(1);
            let is_func_def = if let Some(Token::ParenOpen) = self.lexer.peek_n(paren_idx) {
                let close_idx = self.skip_ws_offset(paren_idx + 1);
                if matches!(self.lexer.peek_n(close_idx), Some(Token::ParenClose)) {
                    // Check if the next non-whitespace token is a brace
                    let mut brace_idx = close_idx + 1;
                    while brace_idx < close_idx + 10
                        && matches!(
                            self.lexer.peek_n(brace_idx),
                            Some(Token::Space | Token::Tab | Token::Comment | Token::Newline)
                        )
                    {
                        brace_idx += 1;
                    }
                    matches!(self.lexer.peek_n(brace_idx), Some(Token::BraceOpen))
                } else {
                    false
                }
            } else {
                false
            };
            if is_func_def {
                parse_posix_function(self)?
            } else {
                // Check if this is a standalone variable assignment: identifier=value or identifier[subscript]=value
                // In bash, a true standalone assignment requires the = to immediately follow
                // the identifier (no whitespace).  "FOO = bar" is a command, not an assignment.

                // Check for simple assignment: identifier=value (no whitespace before =)
                if Self::is_assignment_operator(self.lexer.peek_n(1).cloned())
                    || self.has_indexed_assignment_after_identifier(1)
                {
                    self.parse_standalone_assignment()?
                } else {
                    self.parse_pipeline()?
                }
            }
        } else {
            // A bash KEYWORD immediately followed by `=` is a plain variable
            // ASSIGNMENT, not a keyword construct: `exec=/usr/sbin/dkms`,
            // `export=foo`, `if=1` (bash treats keywords as keywords only
            // when they stand ALONE as a word — `exec = x` with whitespace
            // is the exec builtin). Rewrite the keyword token to an
            // Identifier (its span text — the variable name — is
            // unchanged) so the standalone-assignment path handles the
            // rest identically.
            if Self::is_assignment_operator(self.lexer.peek_n(1).cloned())
                && matches!(
                    self.lexer.peek(),
                    Some(
                        Token::If
                            | Token::Case
                            | Token::While
                            | Token::Until
                            | Token::For
                            | Token::Function
                            | Token::Break
                            | Token::Continue
                            | Token::Return
                            | Token::Shopt
                            | Token::Set
                            | Token::Unset
                            | Token::Export
                            | Token::Readonly
                            | Token::Declare
                            | Token::Typeset
                            | Token::Local
                            | Token::Shift
                            | Token::Eval
                            | Token::Exec
                            | Token::Source
                            | Token::Trap
                            | Token::Wait
                            | Token::Exit
                    )
            ) {
                if let Some((tok, _, _)) = self.lexer.tokens.get_mut(self.lexer.current) {
                    *tok = Token::Identifier;
                }
                self.parse_standalone_assignment()?
            } else {
                match self.lexer.peek() {
                    Some(Token::Comment) => {
                        // Comments should be handled at the top level
                        return Err(ParserError::InvalidSyntax(
                            "Unexpected comment in command parsing".to_string(),
                        ));
                    }
                    Some(Token::If) => parse_if_statement(self)?,
                    Some(Token::Case) => parse_case_statement(self)?,
                    Some(Token::While) => parse_while_loop(self)?,
                    Some(Token::Until) => parse_until_loop(self)?,
                    Some(Token::For) => parse_for_loop(self)?,
                    Some(Token::Function) => parse_function(self)?,
                    Some(Token::Break) => parse_break_statement(self)?,
                    Some(Token::Continue) => parse_continue_statement(self)?,
                    Some(Token::Return) => parse_return_statement(self)?,
                    Some(Token::Shopt) => self.parse_shopt_command()?,
                    // Handle builtin commands
                    Some(Token::Set)
                    | Some(Token::Unset)
                    | Some(Token::Export)
                    | Some(Token::Readonly)
                    | Some(Token::Declare)
                    | Some(Token::Typeset)
                    | Some(Token::Local)
                    | Some(Token::Shift)
                    | Some(Token::Eval)
                    | Some(Token::Exec)
                    | Some(Token::Source)
                    | Some(Token::Trap)
                    | Some(Token::Wait)
                    | Some(Token::Exit) => self.parse_pipeline()?,
                // Handle redirects at the beginning of a command (e.g., process substitution)
                Some(Token::RedirectIn)
                | Some(Token::RedirectOut)
                | Some(Token::RedirectAppend)
                | Some(Token::RedirectInOut)
                | Some(Token::Heredoc)
                | Some(Token::HeredocTabs)
                | Some(Token::HereString)
                | Some(Token::RedirectOutErr)
                | Some(Token::RedirectInErr)
                | Some(Token::RedirectOutClobber)
                | Some(Token::RedirectAll)
                | Some(Token::RedirectAllAppend) => {
                    // Parse as a redirect command with an empty base command
                    let redirects = vec![parse_redirect(&mut self.lexer)?];
                    Command::Redirect(RedirectCommand {
                        command: Box::new(Command::Simple(SimpleCommand {
                            name: Word::literal("".to_string()),
                            args: vec![],
                            redirects: vec![],
                            env_vars: BTreeMap::new(),
                            stdout_used: true,
                            stderr_used: true,
                        })),
                        redirects,
                    })
                }
                // Bash arithmetic evaluation: (( ... ))
                Some(Token::ArithmeticEval) => {
                    self.parse_double_paren_command()?
                }
                Some(Token::ParenOpen) => self.parse_subshell()?,
                Some(Token::BraceOpen) => parse_block(self)?,
                Some(Token::TestBracket) => {
                    // Check for double-bracket test [[ ... ]] before parsing as single bracket
                    if matches!(self.lexer.peek_n(1), Some(Token::TestBracket)) {
                        //                         eprintln!("DEBUG: Found double brackets in parse_command, parsing as test expression");
                        // Consume the first two [[ tokens
                        self.lexer.next();
                        self.lexer.next();
                        let test_command = self.parse_test_expression()?;
                        // After parsing the test expression, check if there's a pipeline operator
                        self.lexer.skip_whitespace_and_comments();
                        let next_token = self.lexer.peek();
                        //                         eprintln!("DEBUG: After test expression, next token: {:?}", next_token);
                        if let Some(token) = next_token {
                            match token {
                                Token::And | Token::Or | Token::Pipe => {
                                    //                                     eprintln!("DEBUG: Found pipeline operator {:?}, parsing as pipeline", token);
                                    // This is part of a pipeline, parse it as such
                                    // For test expressions, we don't need to capture source text
                                    let dummy_start = 0;
                                    let result = self
                                        .parse_pipeline_from_command(test_command, dummy_start)?;
                                    //                                     eprintln!("DEBUG: Pipeline parsing result: {:?}", result);
                                    result
                                }
                                _ => {
                                    //                                     eprintln!("DEBUG: No pipeline operator, returning test expression");
                                    // Just a test expression, return it
                                    test_command
                                }
                            }
                        } else {
                            //                             eprintln!("DEBUG: No more tokens, returning test expression");
                            test_command
                        }
                    } else {
                        // Single bracket test
                        self.parse_test_expression()?
                    }
                }
                Some(Token::Bang) => {
                    // ! at the start of a command is the negation operator
                    // Consume it and parse the rest of the command as a negated pipeline
                    self.lexer.next(); // consume !
                    let cmd = self.parse_pipeline()?;
                    Command::Not(Box::new(cmd))
                }
                Some(Token::ParenClose) => {
                    // A stray `)` outside any subshell (the subshell body
                    // loop pre-checks ParenClose, so reaching here is always
                    // a bash syntax error: bash executes everything BEFORE
                    // it, then aborts with exit 2). Recover it as a literal
                    // `)` command — the ESTree runner's stray-`)` path
                    // (exit 2) then matches bash; the Perl renderer's
                    // handling is best-effort.
                    self.lexer.next();
                    Command::Simple(SimpleCommand {
                        name: Word::literal(")".to_string()),
                        args: vec![],
                        redirects: vec![],
                        env_vars: BTreeMap::new(),
                        stdout_used: true,
                        stderr_used: true,
                    })
                }
                Some(Token::Semicolon) | Some(Token::DoubleSemicolon) => {
                    // Skip semicolon/double-semicolon and continue parsing
                    self.lexer.next();
                    self.parse_command()?
                }
                Some(Token::Pipe) => {
                    // A pipe at the start of a command is a continuation from
                    // a previous line (e.g. after backslash-newline or orphaned |).
                    // Consume it and parse the remaining command as a pipeline segment.
                    self.lexer.next();
                    // Skip whitespace (including newlines) after the pipe
                    self.lexer.skip_whitespace_and_comments();
                    if self.lexer.is_eof() {
                        return Ok(Command::Simple(SimpleCommand {
                            name: Word::literal(String::new()),
                            args: vec![],
                            redirects: vec![],
                            env_vars: BTreeMap::new(),
                            stdout_used: true,
                            stderr_used: true,
                        }));
                    }
                    self.parse_pipeline_segment()?
                }
                Some(Token::Newline) | Some(Token::CarriageReturn) => {
                    // Newlines should be handled at the top level, not here
                    // Return an empty command to indicate we hit a newline
                    self.lexer.next(); // consume the token
                    return Ok(Command::Simple(SimpleCommand {
                        name: Word::literal("".to_string()),
                        args: vec![],
                        redirects: vec![],
                        env_vars: BTreeMap::new(),
                        stdout_used: true,
                        stderr_used: true,
                    }));
                }
                _ => self.parse_pipeline()?,
            }
            }
        };

        let command = self.parse_command_redirects(command)?;

        // Skip only inline whitespace — newlines separate commands.
        self.lexer.skip_inline_whitespace_and_comments();
        if let Some(Token::Background) = self.lexer.peek() {
            self.lexer.next();
            Ok(Command::Background(Box::new(command)))
        } else if matches!(
            self.lexer.peek(),
            Some(Token::And | Token::Or | Token::Pipe)
        ) {
            self.parse_pipeline_from_command(command, 0)
        } else {
            Ok(command)
        }
    }

    fn parse_command_redirects(&mut self, mut command: Command) -> Result<Command, ParserError> {
        // Check if there are redirects following the command
        let mut redirects = Vec::new();

        // Parse redirects until we hit a command separator or other non-redirect token.
        // Skip inline whitespace between redirects so sequences like `cmd <(a) <(b)`
        // keep both operands.  Do NOT skip newlines — they separate commands.
        loop {
            self.lexer.skip_inline_whitespace_and_comments();
            let Some(token) = self.lexer.peek() else {
                break;
            };
            match token {
                Token::Space => {
                    // Skip whitespace between consecutive redirects (e.g. the space
                    // between `<(sort a.txt)` and `<(sort b.txt)` in
                    // `comm -23 <(sort a.txt) <(sort b.txt)`).
                    self.lexer.skip_inline_whitespace_and_comments();
                }
                Token::Number
                | Token::RedirectIn
                | Token::RedirectOut
                | Token::RedirectAppend
                | Token::RedirectInOut
                | Token::Heredoc
                | Token::HeredocTabs
                | Token::HereString
                | Token::RedirectOutErr
                | Token::RedirectInErr
                | Token::RedirectOutClobber
                | Token::RedirectAll
                | Token::RedirectAllAppend => {
                    redirects.push(parse_redirect(&mut self.lexer)?);
                    self.lexer.skip_inline_whitespace_and_comments();
                }
                _ => break,
            }
        }

        // After parsing redirects, collect any additional arguments on the same line.
        // bash allows redirects between arguments (e.g. `grep >/dev/null pattern file`).
        // Only process SimpleCommand and break on keywords that start new statements.
        // NOTE: a `||`/`&&` after a heredoc header is a normal list
        // continuation in bash (verified: `cat <<EOF ||` + body + terminator
        // + `cmd` parses as `cat || cmd` — the right side comes from the
        // line after the terminator; parse-heredoc-or-dangling.sh). The
        // operator token survives the heredoc body re-sync, so it flows to
        // parse_pipeline_from_command below — no special handling here.
        if let Command::Simple(ref mut simple_cmd) = command {
            loop {
                self.lexer.skip_inline_whitespace_and_comments();
                match self.lexer.peek() {
                    Some(Token::Newline) | Some(Token::CarriageReturn)
                    | Some(Token::Semicolon) | Some(Token::DoubleSemicolon)
                    | Some(Token::Pipe) | Some(Token::And) | Some(Token::Or)
                    | Some(Token::Background)
                    | Some(Token::ParenClose) | Some(Token::BraceClose)
                    | Some(Token::Fi) | Some(Token::Then) | Some(Token::Else)
                    | Some(Token::Elif) | Some(Token::Do) | Some(Token::Done)
                    | Some(Token::Esac)
                    // Statement-starting keywords — stop so the caller can
                    // parse them as a new command.
                    | Some(Token::If) | Some(Token::Case) | Some(Token::While)
                    | Some(Token::Until) | Some(Token::For) | Some(Token::Function)
                    // Subshell or block start
                    | Some(Token::ParenOpen) | Some(Token::BraceOpen)
                    | None => break,
                    // Do not consume an Identifier that looks like a function
                    // definition (identifier followed by `()`)
                    // OR the start of a standalone assignment (identifier=value).
                    Some(Token::Identifier) => {
                        let mut pos = 1usize;
                        while matches!(
                            self.lexer.peek_n(pos),
                            Some(Token::Space | Token::Tab | Token::Comment)
                        ) {
                            pos += 1;
                        }
                        // Break if this identifier begins a standalone assignment
                        if Self::is_assignment_operator(self.lexer.peek_n(pos).cloned()) {
                            break;
                        }
                        if matches!(self.lexer.peek_n(pos), Some(Token::ParenOpen))
                            && matches!(self.lexer.peek_n(pos + 1), Some(Token::ParenClose))
                        {
                            break;
                        }
                        simple_cmd.args.push(parse_word_no_newline_skip(&mut self.lexer)?);
                    }
                    // Additional redirects may appear among arguments
                    Some(Token::Number) => {
                        if let Some(next_token) = self.lexer.peek_n(1) {
                            match next_token {
                                Token::RedirectIn | Token::RedirectOut
                                | Token::RedirectAppend | Token::RedirectInErr
                                | Token::RedirectOutErr | Token::RedirectInOut
                                | Token::RedirectAll | Token::RedirectAllAppend
                                | Token::RedirectOutClobber
                                | Token::Heredoc | Token::HeredocTabs
                                | Token::HereString => {
                                    redirects.push(parse_redirect(&mut self.lexer)?);
                                    continue;
                                }
                                _ => {}
                            }
                        }
                        simple_cmd.args.push(parse_word_no_newline_skip(&mut self.lexer)?);
                    }
                    Some(Token::RedirectIn) | Some(Token::RedirectOut)
                    | Some(Token::RedirectAppend) | Some(Token::RedirectInErr)
                    | Some(Token::RedirectOutErr) | Some(Token::RedirectInOut)
                    | Some(Token::RedirectAll) | Some(Token::RedirectAllAppend)
                    | Some(Token::RedirectOutClobber)
                    | Some(Token::Heredoc) | Some(Token::HeredocTabs)
                    | Some(Token::HereString) => {
                        redirects.push(parse_redirect(&mut self.lexer)?);
                        continue;
                    }
                    _ => {
                        simple_cmd.args.push(parse_word_no_newline_skip(&mut self.lexer)?);
                    }
                }
            }
        }

        // Canonicalize combined short flags (`-rf` → `-r -f`) for known
        // flag-taking commands (see parser/normalize.rs). This runs after all
        // arguments — including post-redirect ones — have been collected, and
        // is the single choke point every simple command passes through.
        match &mut command {
            Command::Simple(sc) => {
                if let Word::Literal(name, _) = &sc.name {
                    crate::parser::normalize::normalize_combined_flags(name, &mut sc.args);
                }
            }
            Command::BuiltinCommand(bc) => {
                crate::parser::normalize::normalize_combined_flags(&bc.name, &mut bc.args);
            }
            _ => {}
        }

        if redirects.is_empty() {
            Ok(command)
        } else {
            // Wrap the command with redirects
            Ok(Command::Redirect(RedirectCommand {
                command: Box::new(command),
                redirects,
            }))
        }
    }

    fn is_assignment_operator(token: Option<Token>) -> bool {
        matches!(
            token,
            Some(
                Token::Assign
                    | Token::PlusAssign
                    | Token::MinusAssign
                    | Token::StarAssign
                    | Token::SlashAssign
                    | Token::PercentAssign
            )
        )
    }

    fn has_indexed_assignment_after_identifier(&mut self, start_pos: usize) -> bool {
        if !matches!(self.lexer.peek_n(start_pos), Some(Token::TestBracket)) {
            return false;
        }

        let mut pos = start_pos;
        let mut depth = 0usize;
        while pos < start_pos + 128 {
            match self.lexer.peek_n(pos) {
                Some(Token::TestBracket) => depth += 1,
                Some(Token::TestBracketClose) => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                    if depth == 0 {
                        pos += 1;
                        while pos < start_pos + 128
                            && matches!(
                                self.lexer.peek_n(pos),
                                Some(Token::Space | Token::Tab | Token::Comment | Token::Newline)
                            )
                        {
                            pos += 1;
                        }
                        return Self::is_assignment_operator(self.lexer.peek_n(pos).cloned());
                    }
                }
                Some(_) => {}
                None => return false,
            }
            pos += 1;
        }

        false
    }

    fn parse_assignment_target(&mut self) -> Result<String, ParserError> {
        let mut var_name = self.lexer.get_identifier_text()?;

        match self.lexer.peek() {
            Some(Token::TestBracket) => {
                var_name.push_str(&self.parse_index_suffix()?);
            }
            _ => {}
        }

        Ok(var_name)
    }

    fn parse_index_suffix(&mut self) -> Result<String, ParserError> {
        let mut suffix = String::new();
        let mut depth = 0usize;

        loop {
            match self.lexer.peek() {
                Some(Token::TestBracket) => {
                    // Push the full text of the token (which may include
                    // characters beyond just '[' due to logos fallback
                    // behavior when CasePattern partially matches).
                    if let Some(text) = self.lexer.get_current_text() {
                        suffix.push_str(&text);
                    } else {
                        suffix.push('[');
                    }
                    self.lexer.next();
                    depth += 1;
                }
                Some(Token::TestBracketClose) => {
                    // Similarly push the full text of the closing bracket.
                    if let Some(text) = self.lexer.get_current_text() {
                        suffix.push_str(&text);
                    } else {
                        suffix.push(']');
                    }
                    self.lexer.next();
                    if depth == 0 {
                        return Err(ParserError::InvalidSyntax(
                            "Unbalanced array index brackets".to_string(),
                        ));
                    }
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                Some(_) => suffix.push_str(&self.lexer.get_raw_token_text()?),
                None => {
                    return Err(ParserError::InvalidSyntax(
                        "Unexpected end of input in array index".to_string(),
                    ))
                }
            }
        }

        Ok(suffix)
    }

    fn parse_pipeline_segment(&mut self) -> Result<Command, ParserError> {
        match self.lexer.peek() {
            Some(Token::If) => parse_if_statement(self),
            Some(Token::Case) => parse_case_statement(self),
            Some(Token::While) => parse_while_loop(self),
            Some(Token::Until) => parse_until_loop(self),
            Some(Token::For) => parse_for_loop(self),
            Some(Token::Function) => parse_function(self),
            Some(Token::Break) => parse_break_statement(self),
            Some(Token::Continue) => parse_continue_statement(self),
            Some(Token::Return) => parse_return_statement(self),
            Some(Token::ArithmeticEval) => {
                self.parse_double_paren_command()
            }
            Some(Token::ParenOpen) => self.parse_subshell(),
            Some(Token::BraceOpen) => parse_block(self),
            Some(Token::TestBracket) if matches!(self.lexer.peek_n(1), Some(Token::TestBracket)) => {
                self.lexer.next();
                self.lexer.next();
                self.parse_test_expression()
            }
            Some(Token::TestBracket) => self.parse_test_expression(),
            Some(Token::Bang) => {
                self.lexer.next(); // consume !
                let cmd = self.parse_pipeline_segment()?;
                Ok(Command::Not(Box::new(cmd)))
            }
            // Handle redirects at the beginning of a command (e.g., < "$file")
            Some(Token::RedirectIn)
            | Some(Token::RedirectOut)
            | Some(Token::RedirectAppend)
            | Some(Token::RedirectInOut)
            | Some(Token::Heredoc)
            | Some(Token::HeredocTabs)
            | Some(Token::HereString)
            | Some(Token::RedirectOutErr)
            | Some(Token::RedirectInErr)
            | Some(Token::RedirectOutClobber)
            | Some(Token::RedirectAll)
            | Some(Token::RedirectAllAppend) => {
                let redirects = vec![parse_redirect(&mut self.lexer)?];
                Ok(Command::Redirect(RedirectCommand {
                    command: Box::new(Command::Simple(SimpleCommand {
                        name: Word::literal("".to_string()),
                        args: vec![],
                        redirects: vec![],
                        env_vars: BTreeMap::new(),
                        stdout_used: true,
                        stderr_used: true,
                    })),
                    redirects,
                }))
            }
            Some(Token::Identifier) => {
                // Check for implicit function definition: name() { ... }
                // Allow whitespace between identifier and parentheses
                let paren_idx = self.skip_ws_offset(1);
                let is_func_def = if let Some(Token::ParenOpen) = self.lexer.peek_n(paren_idx) {
                    let close_idx = self.skip_ws_offset(paren_idx + 1);
                    if matches!(self.lexer.peek_n(close_idx), Some(Token::ParenClose)) {
                        let mut brace_idx = close_idx + 1;
                        while brace_idx < close_idx + 10
                            && matches!(
                                self.lexer.peek_n(brace_idx),
                                Some(
                                    Token::Space | Token::Tab | Token::Comment | Token::Newline
                                )
                            )
                        {
                            brace_idx += 1;
                        }
                        matches!(self.lexer.peek_n(brace_idx), Some(Token::BraceOpen))
                    } else {
                        false
                    }
                } else {
                    false
                };
                if is_func_def {
                    return parse_posix_function(self);
                }
                self.parse_simple_command()
            }
            _ => self.parse_simple_command(),
        }
    }

    fn parse_pipeline(&mut self) -> Result<Command, ParserError> {
        // Record the starting byte position for source text capture BEFORE parsing the first command
        let start_span = self.lexer.get_span();
        let start_pos = start_span.map(|(start, _)| start).unwrap_or(0);

        let first_command = self.parse_simple_command()?;
        // Parse redirects for the first command
        let first_command_with_redirects = self.parse_command_redirects(first_command)?;
        self.parse_pipeline_from_command(first_command_with_redirects, start_pos)
    }

    pub fn parse_pipeline_from_command(
        &mut self,
        first_command: Command,
        start_byte_pos: usize,
    ) -> Result<Command, ParserError> {
        // Helper: flush a pipe-commands vec into a Pipeline or a single Command.
        fn flush_pipe_sequence(
            commands: Vec<Command>,
            start_byte_pos: usize,
            parser: &Parser,
        ) -> Command {
            if commands.len() == 1 {
                commands.into_iter().next().unwrap()
            } else {
                let source_text = None; // source_text computed lazily if needed
                Command::Pipeline(Pipeline {
                    commands,
                    source_text,
                    stdout_used: true,
                    stderr_used: true,
                })
            }
        }

        // `pipe_commands` accumulates the current pipe-sequence (connected by `|`).
        let mut pipe_commands = vec![first_command];
        // `result` holds the accumulated logical chain accumulated so far
        // (an And/Or tree), or None if we haven't seen any `&&`/`||` yet.
        let mut result: Option<Command> = None;

        while let Some(_) = self.lexer.peek() {
            // Skip any inline whitespace/comments before checking for an operator
            // Do NOT skip newlines — they separate commands.
            self.lexer.skip_inline_whitespace_and_comments();
            let Some(token) = self.lexer.peek() else {
                break;
            };
            match token {
                Token::Pipe => {
                    self.lexer.next();
                    self.lexer.skip_whitespace_and_comments();
                    let command = self.parse_pipeline_segment()?;
                    // Parse redirects for this command
                    let command_with_redirects = self.parse_command_redirects(command)?;
                    pipe_commands.push(command_with_redirects);
                }
                Token::And | Token::Or => {
                    let is_and = matches!(token, Token::And);
                            self.lexer.next();
                    self.lexer.skip_whitespace_and_comments();
                    if let Some((start, _end)) = self.lexer.get_span() {
                        let (line, col) = self.lexer.offset_to_line_col(start);
                        if crate::debug::is_debug_enabled() {
                            eprintln!("DEBUG After operator, token at {}:{} = {:?}", line, col, self.lexer.peek());
                        }
                    }

                    // Build the left side for this operator:
                    // - If pipe_commands is non-empty, flush it (and optionally wrap
                    //   the previously accumulated result around it).
                    // - If pipe_commands is empty we had a previous `&&`/`||`, so
                    //   the accumulated result IS the left side.
                    let left = if pipe_commands.is_empty() {
                        // result must be Some M-bM-^@M-^T the previous `&&`/`||` stored it
                        result
                            .take()
                            .expect("unexpected empty state in pipeline parsing")
                    } else {
                        let left_pipe = flush_pipe_sequence(pipe_commands, start_byte_pos, self);
                        // Combine with any previously accumulated logical chain
                        if let Some(prev) = result.take() {
                            if is_and {
                                Command::And(Box::new(prev), Box::new(left_pipe))
                            } else {
                                Command::Or(Box::new(prev), Box::new(left_pipe))
                            }
                        } else {
                            left_pipe
                        }
                    };

                    // Parse the right side as a single pipe-sequence (NOT consuming
                    // further `&&`/`||` M-bM-^@M-^T those are handled by the outer loop to
                    // ensure left-associativity).
                    let right_start_span = self.lexer.get_span();
                    let right_start_pos = right_start_span.map(|(s, _)| s).unwrap_or(0);
                    let right_simple = self.parse_pipeline_segment()?;
                    let right_with_redirects = self.parse_command_redirects(right_simple)?;
                    // Only consume `|` here — stop before `&&`/`||`.
                    let mut right_pipe_cmds = vec![right_with_redirects];
                    loop {
                        self.lexer.skip_whitespace_and_comments();
                        if !matches!(self.lexer.peek(), Some(Token::Pipe)) {
                            break;
                        }
                        self.lexer.next(); // consume `|`
                        self.lexer.skip_whitespace_and_comments();
                        let next_simple = self.parse_pipeline_segment()?;
                        let next_with_redirects = self.parse_command_redirects(next_simple)?;
                        right_pipe_cmds.push(next_with_redirects);
                    }
                    let right = flush_pipe_sequence(right_pipe_cmds, right_start_pos, self);

                    // Build the node for THIS operator and store as new result.
                    result = Some(if is_and {
                        Command::And(Box::new(left), Box::new(right))
                    } else {
                        Command::Or(Box::new(left), Box::new(right))
                    });
                    pipe_commands = Vec::new();
                }
                Token::If => {
                    // If we encounter an 'if' token in the middle of a pipeline,
                    // it means we've reached the start of a new command
                    // Break out of the pipeline parsing and let the main parser handle it
                    break;
                }
                Token::Semicolon | Token::Newline => {
                    // Stop parsing pipeline when we hit a command separator
                    break;
                }
                _ => {
                    break;
                }
            }
        }

        // If we have accumulated logical chain, combine with any remaining pipe_commands.
        if let Some(accumulated) = result {
            // pipe_commands should be empty at this point (since the loop always
            // clears pipe_commands after building a logical node), but guard anyway.
            if pipe_commands.is_empty() {
                return Ok(accumulated);
            }
            let remaining = flush_pipe_sequence(pipe_commands, start_byte_pos, self);
            // The remaining pipe_commands appear after the last `&&`/`||` — this
            // shouldn't normally happen, but if it does, combine as a pipeline.
            return Ok(Command::Pipeline(Pipeline {
                commands: vec![accumulated, remaining],
                source_text: None,
                stdout_used: true,
                stderr_used: true,
            }));
        }

        if pipe_commands.len() == 1 {
            let result = pipe_commands.remove(0);
            Ok(result)
        } else {
            // Capture the source text from start to current position
            let end_span = self.lexer.get_span();
            let end_byte_pos = end_span.map(|(_, end)| end).unwrap_or(start_byte_pos);
            let source_text = if start_byte_pos < end_byte_pos {
                // Get the text from the lexer's input
                let text = self.lexer.get_text(start_byte_pos, end_byte_pos);
                Some(text.trim().to_string())
            } else {
                None
            };

            let result = Command::Pipeline(Pipeline {
                commands: pipe_commands,
                source_text,
                stdout_used: true,
                stderr_used: true,
            });
            Ok(result)
        }
    }

    fn rebalance_logical_chain(command: Command) -> Command {
        fn collect(command: Command, operands: &mut Vec<Command>, operators: &mut Vec<bool>) {
            match command {
                Command::And(left, right) => {
                    collect(*left, operands, operators);
                    operators.push(true);
                    collect(*right, operands, operators);
                }
                Command::Or(left, right) => {
                    collect(*left, operands, operators);
                    operators.push(false);
                    collect(*right, operands, operators);
                }
                other => operands.push(other),
            }
        }

        let mut operands = Vec::new();
        let mut operators = Vec::new();
        collect(command, &mut operands, &mut operators);

        let mut operands = operands.into_iter();
        let mut expr = operands
            .next()
            .expect("logical chain should contain at least one operand");

        for (is_and, operand) in operators.into_iter().zip(operands) {
            expr = if is_and {
                Command::And(Box::new(expr), Box::new(operand))
            } else {
                Command::Or(Box::new(expr), Box::new(operand))
            };
        }

        expr
    }

    pub fn parse_simple_command(&mut self) -> Result<Command, ParserError> {
        // Skip whitespace and comments at the beginning
        self.lexer.skip_whitespace_and_comments();

        // Check if this is a test expression first
        if matches!(self.lexer.peek(), Some(Token::TestBracket)) {
            if matches!(self.lexer.peek_n(1), Some(Token::TestBracket)) {
                // Double bracket [[ ]] - consume both opening brackets before parsing
                self.lexer.next(); // consume first [
                self.lexer.next(); // consume second [
                                   // parse_test_expression will detect is_double_bracket=true since current token is not TestBracket
                return self.parse_test_expression();
            }
            return self.parse_test_expression();
        }

        let mut args = Vec::new();
        let redirects = Vec::new();
        let mut env_vars = BTreeMap::new();

        // Parse environment variable-style assignments at the start
        while let Some(token) = self.lexer.peek() {
            match token {
                Token::Identifier => {
                    let compound_op = if Self::is_assignment_operator(self.lexer.peek_n(1).cloned())
                    {
                        self.lexer.peek_n(1).cloned()
                    } else if self.has_indexed_assignment_after_identifier(1) {
                        let mut pos = 1usize;
                        let mut depth = 0usize;
                        loop {
                            match self.lexer.peek_n(pos) {
                                Some(Token::TestBracket) => depth += 1,
                                Some(Token::TestBracketClose) => {
                                    depth -= 1;
                                    if depth == 0 {
                                        pos += 1;
                                        break;
                                    }
                                }
                                Some(_) => {},
                                None => break,
                            }
                            pos += 1;
                        }
                        while matches!(
                            self.lexer.peek_n(pos),
                            Some(Token::Space | Token::Tab | Token::Comment | Token::Newline)
                        ) {
                            pos += 1;
                        }
                        self.lexer.peek_n(pos).cloned()
                    } else if matches!(self.lexer.peek_n(1), Some(Token::TestBracket)) {
                        // identifier[subscript]op: position 1=TestBracket([), 2=content, 3=TestBracketClose(]), 4=op
                        self.lexer.peek_n(4).cloned()
                    } else {
                        None
                    };

                    match compound_op {
                        Some(Token::PlusAssign) => {
                            let var_name = self.parse_assignment_target()?;
                            self.lexer.next(); // consume +=
                            if matches!(self.lexer.peek(), Some(Token::ParenOpen)) {
                                let elements = parse_array_elements(&mut self.lexer)?;
                                let array_word = Word::array(var_name.clone(), elements);
                                env_vars.insert(var_name, array_word);
                            } else {
                                let value_word = parse_word(&mut self.lexer)?;
                                let arithmetic_expr =
                                    format!("{}+{}", var_name, value_word.to_string());
                                let compound_word = Word::arithmetic(ArithmeticExpression {
                                    expression: arithmetic_expr,
                                    tokens: vec![],
                                });
                                env_vars.insert(var_name, compound_word);
                            }
                            self.lexer.skip_inline_whitespace_and_comments();
                        }
                        Some(Token::Assign) => {
                            let var_name = self.parse_assignment_target()?;
                            self.lexer.next(); // consume =
                            if matches!(self.lexer.peek(), Some(Token::ParenOpen)) {
                                let elements = parse_array_elements(&mut self.lexer)?;
                                let array_word = Word::array(var_name.clone(), elements);
                                env_vars.insert(var_name, array_word);
                            } else if matches!(
                                self.lexer.peek(),
                                Some(
                                    Token::Space
                                        | Token::Tab
                                        | Token::Newline
                                        | Token::CarriageReturn
                                        | Token::Semicolon
                                )
                                | None
                            ) {
                                // Empty value (e.g. IFS=)
                                env_vars.insert(var_name, Word::literal(String::new()));
                            } else {
                                let value_word = parse_word(&mut self.lexer)?;
                                env_vars.insert(var_name, value_word);
                            }
                            self.lexer.skip_inline_whitespace_and_comments();
                        }
                        Some(Token::MinusAssign)
                        | Some(Token::StarAssign)
                        | Some(Token::SlashAssign)
                        | Some(Token::PercentAssign)
                        | None => {
                            break;
                        }
                        _ => break,
                    }
                }
                _ => break,
            }
        }

        // Check for implicit function definition: identifier() { ... }
        // This must happen before parsing the command name, so we can
        // delegate to parse_posix_function while the identifier is still
        // the current token.
        if let Some(Token::Identifier) = self.lexer.peek() {
            // Allow whitespace between identifier and parentheses
            let paren_idx = self.skip_ws_offset(1);
            let is_func_def = if let Some(Token::ParenOpen) = self.lexer.peek_n(paren_idx) {
                let close_idx = self.skip_ws_offset(paren_idx + 1);
                if matches!(self.lexer.peek_n(close_idx), Some(Token::ParenClose)) {
                    let mut brace_idx = close_idx + 1;
                    while brace_idx < close_idx + 10
                        && matches!(
                            self.lexer.peek_n(brace_idx),
                            Some(Token::Space | Token::Tab | Token::Comment | Token::Newline)
                        )
                    {
                        brace_idx += 1;
                    }
                    matches!(self.lexer.peek_n(brace_idx), Some(Token::BraceOpen))
                } else {
                    false
                }
            } else {
                false
            };
            if is_func_def {
                // Wrap env var assignments and the function in a block
                if !env_vars.is_empty() {
                    let func = parse_posix_function(self)?;
                    let mut commands = Vec::new();
                    for (var_name, value) in env_vars {
                        commands.push(Command::Assignment(Assignment {
                            variable: var_name,
                            value,
                            operator: AssignmentOperator::Assign,
                        }));
                    }
                    commands.push(func);
                    return Ok(Command::Block(Block { commands }));
                }
                return parse_posix_function(self);
            }
        }

        // If there are env vars and the next token is not a valid command name,
        // return these as standalone assignments instead of trying to parse
        // a command name that doesn't exist.
        if !env_vars.is_empty() {
            let is_command_following = matches!(
                self.lexer.peek(),
                Some(Token::Identifier)
                    | Some(Token::DoubleQuotedString)
                    | Some(Token::SingleQuotedString)
                    | Some(Token::Dollar)
                    | Some(Token::DollarBrace)
                    | Some(Token::DollarParen)
                    | Some(Token::BacktickString)
                    | Some(Token::Arithmetic)
                    | Some(Token::ArithmeticEval)
                    | Some(Token::TestBracket)
                    | Some(Token::Bang)
                    | Some(Token::Tilde)
                    | Some(Token::Number)
                    | Some(Token::Float)
                    | Some(Token::PaddedNumber)
                    | Some(Token::HexNumber)
                    | Some(Token::Slash)
                    | Some(Token::Dot)
                    | Some(Token::Range)
                    | Some(Token::Minus)
                    | Some(Token::Plus)
                    | Some(Token::Star)
                    | Some(Token::Percent)
                    | Some(Token::Escape)
                    | Some(Token::EscapedDoubleQuote) | Some(Token::EscapedSingleQuote) | Some(Token::EscapedBacktick)
                    | Some(Token::Colon)
                    | Some(Token::Comma)
                    | Some(Token::If)
                    | Some(Token::Case)
                    | Some(Token::While)
                    | Some(Token::Until)
                    | Some(Token::For)
                    | Some(Token::Function)
                    | Some(Token::BraceOpen)
                    | Some(Token::ParenOpen)
                    | Some(Token::Set)
                    | Some(Token::Unset)
                    | Some(Token::Export)
                    | Some(Token::Readonly)
                    | Some(Token::Declare)
                    | Some(Token::Typeset)
                    | Some(Token::Local)
                    | Some(Token::Shift)
                    | Some(Token::Eval)
                    | Some(Token::Exec)
                    | Some(Token::Source)
                    | Some(Token::Trap)
                    | Some(Token::Wait)
                    | Some(Token::Exit)
                    | Some(Token::True)
                    | Some(Token::False)
            );
            if !is_command_following {
                // No command following - return standalone assignments
                let mut commands: Vec<Command> = env_vars.into_iter().map(|(variable, value)| {
                    Command::Assignment(Assignment {
                        variable,
                        value,
                        operator: AssignmentOperator::Assign,
                    })
                }).collect();
                if commands.len() == 1 {
                    return Ok(commands.remove(0));
                }
                return Ok(Command::Block(Block { commands }));
            }
        }

        // If the first token is BraceOpen, this is a block command { ... }, not a word
        if matches!(self.lexer.peek(), Some(Token::BraceOpen)) {
            return parse_block(self);
        }

        // Parse the command name first
        let name = parse_word(&mut self.lexer)?;
        let name = match name {
            Word::StringInterpolation(interp, ann) if interp.parts.len() == 1 => {
                if let Some(StringPart::Literal(literal)) = interp.parts.first() {
                    Word::Literal(literal.clone(), ann)
                } else {
                    Word::StringInterpolation(interp, ann)
                }
            }
            other => other,
        };

        // Skip inline whitespace before parsing arguments (but stop at newlines)
        self.lexer.skip_inline_whitespace_and_comments();

        // Check if this is a builtin command
        if let Word::Literal(name_str, _) = &name {
            if is_builtin_command(&name_str) {
                // Special handling for local/declare/typeset/export command with assignments
                if matches!(
                    name_str.as_str(),
                    "local" | "declare" | "typeset" | "export"
                ) {
                    // Parse local/declare assignments like: local var=value, declare -a arr=(...)
                    // Stop at newlines to handle multiple local commands on separate lines
                    while let Some(token) = self.lexer.peek() {
                        match token {
                            Token::Space | Token::Tab | Token::Comment => {
                                self.lexer.next();
                                continue;
                            }
                            Token::Newline | Token::CarriageReturn => {
                                // Stop parsing arguments at newlines to allow separate local commands
                                break;
                            }
                            Token::Local => {
                                // This is the start of a new local command, stop parsing this one
                                break;
                            }
                            Token::Identifier => {
                                // Check if this is an assignment: var=value
                                if matches!(self.lexer.peek_n(1), Some(Token::Assign)) {
                                    let var_name = self.lexer.get_identifier_text()?;
                                    self.lexer.next(); // consume =

                                    // Handle array initialization: var=(elem1 elem2 ...)
                                    if matches!(self.lexer.peek(), Some(Token::ParenOpen)) {
                                        // Use parse_array_elements which consumes the parens and returns Vec<String>
                                        let elements = parse_array_elements(&mut self.lexer)?;
                                        args.push(Word::Array(var_name, elements, None));
                                    } else {
                                        // Handle different types of values after =
                                        let value_word = match self.lexer.peek() {
                                            Some(Token::Dollar) => {
                                                // Handle $1, $2, $variable, etc.
                                                self.lexer.next(); // consume $
                                                match self.lexer.peek() {
                                                    Some(Token::Number) => {
                                                        // get_number_text already advances the lexer
                                                        let num = self.lexer.get_number_text()?;
                                                        Word::Literal(format!("${}", num), None)
                                                    }
                                                    Some(Token::Identifier) => {
                                                        // get_identifier_text already advances the lexer
                                                        let var_name =
                                                            self.lexer.get_identifier_text()?;
                                                        Word::Literal(
                                                            format!("${}", var_name),
                                                            None,
                                                        )
                                                    }
                                                    _ => {
                                                        return Err(ParserError::InvalidSyntax("Expected identifier or number after $ in local assignment".to_string()));
                                                    }
                                                }
                                            }
                                            _ => {
                                                // Check for empty value (e.g. export ENV=)
                                                if matches!(
                                                    self.lexer.peek(),
                                                    Some(
                                                        Token::Space
                                                            | Token::Tab
                                                            | Token::Newline
                                                            | Token::CarriageReturn
                                                            | Token::Semicolon
                                                    )
                                                    | None
                                                ) {
                                                    Word::literal(String::new())
                                                } else {
                                                    // For other types, use parse_word
                                                    parse_word(&mut self.lexer)?
                                                }
                                            }
                                        };

                                        // Create assignment word: var=value
                                        // Handle command substitutions and complex expansions properly
                                        // by storing them as separate args so the generator can handle them correctly
                                        let assignment_word = match &value_word {
                                            Word::CommandSubstitution(_, _)
                                            | Word::ParameterExpansion(_, _)
                                            | Word::StringInterpolation(_, _)
                                            | Word::Variable(_, _, _)
                                            | Word::Arithmetic(_, _) => {
                                                Word::Literal(format!("{}=", var_name), None)
                                            }
                                            _ => Word::Literal(
                                                format!(
                                                    "{}={}",
                                                    var_name,
                                                    value_word
                                                        .as_literal()
                                                        .unwrap_or(&value_word.to_string())
                                                ),
                                                None,
                                            ),
                                        };
                                        args.push(assignment_word);

                                        // If the value is a complex type, add it as a separate argument
                                        match &value_word {
                                            Word::CommandSubstitution(cmd, _) => {
                                                args.push(Word::CommandSubstitution(
                                                    cmd.clone(), None,
                                                ));
                                            }
                                            Word::ParameterExpansion(pe, _) => {
                                                args.push(Word::ParameterExpansion(
                                                    pe.clone(), None,
                                                ));
                                            }
                                            Word::StringInterpolation(si, _) => {
                                                args.push(Word::StringInterpolation(
                                                    si.clone(), None,
                                                ));
                                            }
                                            Word::Variable(v, _, _) => {
                                                args.push(Word::Variable(
                                                    v.clone(), true, None,
                                                ));
                                            }
                                            Word::Arithmetic(arith_expr, _) => {
                                                args.push(Word::Arithmetic(
                                                    arith_expr.clone(), None,
                                                ));
                                            }
                                            _ => {}
                                        }
                                    }
                                } else {
                                    // If not an assignment, check if this is the start of a
                                    // new local/builtin command (e.g. a second `local` on the
                                    // next line whose leading newline was already consumed).
                                    if let Some(text) = self.lexer.get_current_text() {
                                        if text == "local" || text == "declare" || text == "export"
                                        {
                                            break;
                                        }
                                    }
                                    args.push(parse_word_no_newline_skip(&mut self.lexer)?);
                                }
                            }
                            Token::Pipe
                            | Token::And
                            | Token::Or
                            | Token::Semicolon
                            | Token::DoubleSemicolon
                            | Token::Background
                            | Token::RedirectIn
                            | Token::RedirectOut
                            | Token::RedirectAppend
                            | Token::RedirectInErr
                            | Token::RedirectOutErr
                            | Token::RedirectInOut
                            | Token::RedirectAll
                            | Token::RedirectAllAppend
                            | Token::Heredoc
                            | Token::HeredocTabs
                            | Token::HereString => break,
                            _ => {
                                args.push(parse_word_no_newline_skip(&mut self.lexer)?);
                            }
                        }
                    }

                    return Ok(Command::BuiltinCommand(BuiltinCommand {
                        name: name_str.clone(),
                        args,
                        redirects,
                        env_vars,
                        stdout_used: true,
                        stderr_used: true,
                    }));
                }

                // Parse as builtin command
                while let Some(token) = self.lexer.peek() {
                    match token {
                        Token::Space | Token::Tab | Token::Comment => {
                            // Skip inline whitespace and comments, but continue parsing arguments
                            self.lexer.next();
                            continue;
                        }
                        Token::Newline | Token::CarriageReturn => {
                            // Newlines should break argument parsing as they separate commands
                            break;
                        }
                        Token::ParenClose => {
                            // Stop parsing arguments when we hit a closing parenthesis
                            break;
                        }
                        Token::RedirectIn
                        | Token::RedirectOut
                        | Token::RedirectAppend
                        | Token::RedirectInErr
                        | Token::RedirectOutErr
                        | Token::RedirectInOut
                        | Token::RedirectAll
                        | Token::RedirectAllAppend
                        | Token::Heredoc
                        | Token::HeredocTabs
                        | Token::HereString => {
                            break;
                        }
                        Token::Number => {
                            // Check if this number is followed by a redirect operator (file descriptor redirection)
                            if let Some(next_token) = self.lexer.peek_n(1) {
                                match next_token {
                                    Token::RedirectIn
                                    | Token::RedirectOut
                                    | Token::RedirectAppend
                                    | Token::RedirectInErr
                                    | Token::RedirectOutErr
                                    | Token::RedirectInOut
                                    | Token::RedirectAll
                                    | Token::RedirectAllAppend
                                    | Token::Heredoc
                                    | Token::HeredocTabs
                                    | Token::HereString => {
                                        // This is a file descriptor redirection, break out of argument parsing
                                        break;
                                    }
                                    _ => {
                                        // This is just a regular number argument
                                        args.push(parse_word_no_newline_skip(&mut self.lexer)?);
                                    }
                                }
                            } else {
                                // No next token, treat as regular number argument
                                args.push(parse_word_no_newline_skip(&mut self.lexer)?);
                            }
                        }
                        Token::Pipe
                        | Token::And
                        | Token::Or
                        | Token::Semicolon
                        | Token::DoubleSemicolon
                        | Token::Background => {
                            break;
                        }
                        _ => {
                            // For any other token, try to parse it as a word
                            args.push(parse_word_no_newline_skip(&mut self.lexer)?);
                        }
                    }
                }

                return Ok(Command::BuiltinCommand(BuiltinCommand {
                    name: name_str.clone(),
                    args,
                    redirects,
                    env_vars,
                    stdout_used: true,
                    stderr_used: true,
                }));
            }
        }

        // Special handling for Bash single-bracket test: capture everything until closing ']'
        if let Word::Literal(name_str, _) = &name {
            if name_str == "[" {
                let expr = self.lexer.capture_single_bracket_expression()?;
                args.push(Word::literal(expr));
            }
        }

        // Parse arguments
        while let Some(token) = self.lexer.peek() {
            match token {
                Token::Space | Token::Tab | Token::Comment => {
                    // Skip inline whitespace and comments, but continue parsing arguments
                    self.lexer.next();
                    continue;
                }
                Token::Newline | Token::CarriageReturn => {
                    // Newlines should break argument parsing as they separate commands
                    break;
                }
                Token::ParenClose => {
                    // Stop parsing arguments when we hit a closing parenthesis
                    break;
                }
                Token::RedirectIn
                | Token::RedirectOut
                | Token::RedirectAppend
                | Token::RedirectInErr
                | Token::RedirectOutErr
                | Token::RedirectInOut
                | Token::RedirectAll
                | Token::RedirectAllAppend
                | Token::RedirectOutClobber
                | Token::Heredoc
                | Token::HeredocTabs
                | Token::HereString => {
                    break;
                }
                Token::Number => {
                    // Check if this number is followed by a redirect operator (file descriptor redirection)
                    if let Some(next_token) = self.lexer.peek_n(1) {
                        match next_token {
                            Token::RedirectIn
                            | Token::RedirectOut
                            | Token::RedirectAppend
                            | Token::RedirectInErr
                            | Token::RedirectOutErr
                            | Token::RedirectInOut
                            | Token::RedirectAll
                            | Token::RedirectAllAppend
                            | Token::RedirectOutClobber
                            | Token::Heredoc
                            | Token::HeredocTabs
                            | Token::HereString => {
                                // This is a file descriptor redirection, break out of argument parsing
                                break;
                            }
                            _ => {
                                // This is just a regular number argument
                                args.push(parse_word_no_newline_skip(&mut self.lexer)?);
                            }
                        }
                    } else {
                        // No next token, treat as regular number argument
                        args.push(parse_word_no_newline_skip(&mut self.lexer)?);
                    }
                }
                Token::Pipe
                | Token::And
                | Token::Or
                | Token::Semicolon
                | Token::DoubleSemicolon
                | Token::Background => {
                    break;
                }
                Token::Character
                | Token::NonZero
                | Token::Exists
                | Token::File
                | Token::Size
                | Token::Readable
                | Token::Writable
                | Token::Executable
                | Token::NewerThan
                | Token::OlderThan
                | Token::NameFlag
                | Token::MaxDepthFlag
                | Token::TypeFlag
                | Token::Plus
                | Token::Minus
                | Token::Escape
                | Token::EscapedDoubleQuote | Token::EscapedSingleQuote | Token::EscapedBacktick => {
                    // These are valid argument tokens
                    args.push(parse_word_no_newline_skip(&mut self.lexer)?);

                    // If this is a flag that takes an argument, continue parsing to get the argument
                    if let Word::Literal(arg_str, _) = args.last().unwrap() {
                        if arg_str == "-name" || arg_str == "-maxdepth" || arg_str == "-type" {
                            // Skip whitespace and comments
                            self.lexer.skip_whitespace_and_comments();

                            // Check if the next token is a valid argument to the flag
                            if let Some(next_token) = self.lexer.peek() {
                                match next_token {
                                    Token::Identifier
                                    | Token::DoubleQuotedString
                                    | Token::SingleQuotedString => {
                                        // This is an argument to the flag, parse it
                                        args.push(parse_word_no_newline_skip(&mut self.lexer)?);
                                    }
                                    _ => {
                                        // Not an argument to the flag, continue
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {
                    // Check if this token should break out of argument parsing
                    match token {
                        Token::Pipe | Token::And | Token::Or => {
                            // Pipeline operators should break argument parsing
                            break;
                        }
                        Token::Identifier => {
                            // Check if we're at a newline boundary - if so, this identifier
                            // might be the start of a new command, not an argument
                            let _current_pos = self.lexer.get_position();

                            // Look backwards to see if there was a newline before this identifier
                            // This is a heuristic to detect command boundaries
                            if self.lexer.has_newline_before_current_token() {
                                // This identifier is likely the start of a new command
                                break;
                            }

                            // Otherwise, treat it as an argument
                            args.push(parse_word_no_newline_skip(&mut self.lexer)?);
                        }
                        _ => {
                            // For any other token, try to parse it as a word
                            // This handles cases like quoted strings, etc.
                            args.push(parse_word_no_newline_skip(&mut self.lexer)?);
                        }
                    }
                }
            }
        }

        Ok(Command::Simple(SimpleCommand {
            name,
            args,
            redirects,
            env_vars,
            stdout_used: true,
            stderr_used: true,
        }))
    }

    fn parse_standalone_assignment(&mut self) -> Result<Command, ParserError> {
        // Get the variable name - this could be a simple identifier or an array access like map[foo]
        let var_name = self.parse_assignment_target()?;

        // Consume the assignment token (=, +=, -=, etc.)
        // Skip inline whitespace before the operator (defensive; the caller
        // should have ensured no whitespace before `=`, but arrays like
        // `arr=( ... )` may have whitespace between the var name and `=`
        // when parsed via the env-var path in parse_simple_command).
        self.lexer.skip_inline_whitespace_and_comments();
        let assignment_op = self.lexer.peek().cloned().unwrap();
        match assignment_op {
            Token::Assign
            | Token::PlusAssign
            | Token::MinusAssign
            | Token::StarAssign
            | Token::SlashAssign
            | Token::PercentAssign => {
                self.lexer.next();
            }
            _ => {
                return Err(ParserError::InvalidSyntax(
                    "Expected assignment operator".to_string(),
                ))
            }
        }

        // Parse the value
        let value_word = if matches!(self.lexer.peek(), Some(Token::ParenOpen)) {
            // This is an array assignment like arr=(one two three)
            let elements = parse_array_elements(&mut self.lexer)?;
            Word::array(var_name.clone(), elements)
        } else if matches!(
            self.lexer.peek(),
            Some(
                Token::Space
                    | Token::Tab
                    | Token::Newline
                    | Token::CarriageReturn
                    | Token::Semicolon
                    | Token::DoubleSemicolon
            )
            | None
        ) {
            // Empty value (e.g. IFS= read ...)
            Word::literal(String::new())
        } else {
            let mut value_word = parse_word(&mut self.lexer)?;
            // After parsing the initial value, check if there are adjacent
            // tokens (no whitespace gap) that should be part of the same value.
            // This handles patterns like `var='foo'\'bar'` where the lexer
            // splits the shell idiom for embedding a single quote into multiple
            // tokens (SingleQuotedString + Escape + SingleQuotedString + ...).
            //
            // Only activate the merge loop when the next adjacent token is a
            // continuation (Escape, bare SingleQuote, or another quoted string)
            // rather than a separator (Newline, Space).
            let should_merge = match self.lexer.peek() {
                Some(Token::Escape) => true,
                Some(Token::EscapedSingleQuote) => true,
                Some(Token::SingleQuote) => true,
                Some(Token::SingleQuotedString) => true,
                Some(Token::Newline) => {
                    // Only continue across newlines if the first value token
                    // was a SingleQuotedString that did NOT reach the closing
                    // quote on the same line (i.e. the next token after the
                    // string is not Newline/Space).
                    false
                }
                _ => false,
            };
            if should_merge {
                loop {
                    let next_token = match self.lexer.peek() {
                        Some(t) => t,
                        None => break,
                    };
                    // Stop on token types that can never be part of a value.
                    if matches!(
                        next_token,
                        Token::Space
                            | Token::Tab
                            | Token::CarriageReturn
                            | Token::Semicolon
                            | Token::DoubleSemicolon
                            | Token::Background
                            | Token::Pipe
                            | Token::And
                            | Token::Or
                            | Token::RedirectIn
                            | Token::RedirectOut
                            | Token::RedirectAppend
                            | Token::Heredoc
                            | Token::HeredocTabs
                            | Token::HereString
                            | Token::RedirectInOut
                            | Token::RedirectOutErr
                            | Token::RedirectInErr
                            | Token::RedirectOutClobber
                            | Token::RedirectAll
                            | Token::RedirectAllAppend
                    ) {
                        break;
                    }
                    // Check if the next token is adjacent (no gap)
                    let prev_end = match self.lexer.tokens.get(self.lexer.current.saturating_sub(1)) {
                        Some((_, _, end)) => *end,
                        None => break,
                    };
                    let next_start = match self.lexer.tokens.get(self.lexer.current) {
                        Some((_, start, _)) => *start,
                        None => break,
                    };
                    if next_start != prev_end {
                        break;
                    }
                    // Bare SingleQuote = closing delimiter
                    if matches!(next_token, Token::SingleQuote) {
                        self.lexer.next();
                        break;
                    }
                    // EscapedSingleQuote token (\') -> literal single quote
                    if matches!(next_token, Token::EscapedSingleQuote) {
                        self.lexer.next(); // consume the EscapedSingleQuote token
                        let mut parts = word_to_parts(value_word);
                        parts.push(StringPart::Literal("'".to_string()));
                        value_word = Word::StringInterpolation(StringInterpolation { parts }, None);
                        continue;
                    }
                    // Escape + character: consume the escape and output only
                    // the escaped character (e.g. \' produces just ').
                    if matches!(next_token, Token::Escape) {
                        self.lexer.next(); // consume the backslash
                        // The next token is the escaped character.
                        if let Some(escaped_text) = self.lexer.get_current_text() {
                            let mut parts = word_to_parts(value_word);
                            // For a SingleQuotedString token (like ''),
                            // strip the outer quotes and use the content.
                            let inner = if (escaped_text.starts_with('\'') && escaped_text.ends_with('\''))
                                || (escaped_text.starts_with('"') && escaped_text.ends_with('"'))
                            {
                                &escaped_text[1..escaped_text.len()-1]
                            } else {
                                &escaped_text[..]
                            };
                            if inner.is_empty() {
                                // Empty content (e.g. '' after escape) means
                                // the escaped char is just '
                                parts.push(StringPart::Literal("'".to_string()));
                            } else {
                                parts.push(StringPart::Literal(inner.to_string()));
                            }
                            value_word = Word::StringInterpolation(StringInterpolation { parts }, None);
                            self.lexer.next();
                            continue;
                        }
                        break;
                    }
                    // Newline is literal content inside a multi-line string,
                    // but only if we are still inside a quoted region. A newline
                    // that follows a SingleQuotedString (which ends with ') is a
                    // command separator, not string content.
                    if matches!(next_token, Token::Newline) {
                        // Check the previous token to see if we're inside a
                        // quoted region.
                        let prev_is_quote_end = self.lexer.current >= 2
                            && matches!(
                                self.lexer.tokens.get(self.lexer.current - 1),
                                Some((Token::SingleQuotedString, _, _))
                            );
                        if prev_is_quote_end {
                            // This newline follows a closing ' — it's a command
                            // separator, not string content.
                            break;
                        }
                        value_word = {
                            let mut parts = word_to_parts(value_word);
                            parts.push(StringPart::Literal("\n".to_string()));
                            Word::StringInterpolation(StringInterpolation { parts }, None)
                        };
                        self.lexer.next();
                        continue;
                    }
                    // Next token is adjacent — parse it and combine
                    let next_word = parse_word(&mut self.lexer)?;
                    let mut parts = word_to_parts(value_word);
                    parts.extend(word_to_parts(next_word));
                    value_word = Word::StringInterpolation(StringInterpolation { parts }, None);
                }
            }
            value_word
        };

        // Check if there's a command following this assignment.
        // Use skip_inline to NOT skip newlines — a newline separates
        // the assignment from any following command.
        self.lexer.skip_inline_whitespace_and_comments();

        /// Returns true if the token can start a simple command name
        /// (an identifier or a builtin/reserved-word keyword).
        fn is_command_name_token(token: &Token) -> bool {
            matches!(
                token,
                Token::Identifier
                    | Token::If
                    | Token::Then
                    | Token::Else
                    | Token::Elif
                    | Token::Fi
                    | Token::While
                    | Token::Until
                    | Token::For
                    | Token::Do
                    | Token::Done
                    | Token::In
                    | Token::Function
                    | Token::Case
                    | Token::Esac
                    | Token::Select
                    | Token::Break
                    | Token::Continue
                    | Token::Return
                    | Token::Exit
                    | Token::Export
                    | Token::Readonly
                    | Token::Local
                    | Token::Declare
                    | Token::Typeset
                    | Token::Unset
                    | Token::Shift
                    | Token::Eval
                    | Token::Exec
                    | Token::Source
                    | Token::Trap
                    | Token::Wait
                    | Token::Set
                    | Token::Shopt
                    | Token::True
                    | Token::False
                    | Token::TestBracket
                    | Token::Bang
                    | Token::ParenOpen
                    | Token::BraceOpen
                    | Token::ArithmeticEval
            )
        }

        // Convert the assignment operator token to an AssignmentOperator.
        let op_to_assignop = |tok: &Token| -> AssignmentOperator {
            match tok {
                Token::PlusAssign => AssignmentOperator::PlusAssign,
                Token::MinusAssign => AssignmentOperator::MinusAssign,
                Token::StarAssign => AssignmentOperator::StarAssign,
                Token::SlashAssign => AssignmentOperator::SlashAssign,
                Token::PercentAssign => AssignmentOperator::PercentAssign,
                _ => AssignmentOperator::Assign,
            }
        };
        let first_op = op_to_assignop(&assignment_op);

        // Check if there are more consecutive assignments before a command.
        // Collect all identifier=value pairs before checking for a following command.
        let mut env_vars: BTreeMap<String, Word> = BTreeMap::new();
        let mut env_ops: BTreeMap<String, AssignmentOperator> = BTreeMap::new();
        env_ops.insert(var_name.clone(), first_op);
        env_vars.insert(var_name, value_word);

        // Loop to parse additional consecutive env-var assignments
        loop {
            self.lexer.skip_inline_whitespace_and_comments();
            match self.lexer.peek() {
                Some(Token::Identifier) => {
                    let mut pos = 1usize;
                    while matches!(
                        self.lexer.peek_n(pos),
                        Some(Token::Space | Token::Tab | Token::Comment)
                    ) {
                        pos += 1;
                    }
                    let is_next_assignment =
                        Self::is_assignment_operator(self.lexer.peek_n(pos).cloned())
                        || self.has_indexed_assignment_after_identifier(pos);
                    if is_next_assignment {
                        // Parse the next assignment
                        let next_var = self.parse_assignment_target()?;
                        let next_op = self.lexer.peek().cloned().unwrap();
                        let next_op_converted = op_to_assignop(&next_op);
                        match next_op {
                            Token::Assign
                            | Token::PlusAssign
                            | Token::MinusAssign
                            | Token::StarAssign
                            | Token::SlashAssign
                            | Token::PercentAssign => {
                                self.lexer.next();
                            }
                            _ => {
                                return Err(ParserError::InvalidSyntax(
                                    "Expected assignment operator".to_string(),
                                ))
                            }
                        }
                        let next_value = if matches!(self.lexer.peek(), Some(Token::ParenOpen)) {
                            let elements = parse_array_elements(&mut self.lexer)?;
                            Word::array(next_var.clone(), elements)
                        } else if matches!(
                            self.lexer.peek(),
                            Some(
                                Token::Space
                                    | Token::Tab
                                    | Token::Newline
                                    | Token::CarriageReturn
                                    | Token::Semicolon
                                    | Token::DoubleSemicolon
                            )
                            | None
                        ) {
                            Word::literal(String::new())
                        } else {
                            parse_word(&mut self.lexer)?
                        };
                        env_vars.insert(next_var.clone(), next_value);
                        env_ops.insert(next_var, next_op_converted);
                        continue;
                    }
                    break;
                }
                _ => break,
            }
        }

        // Check if there's a command following this assignment.
        // Use skip_inline to NOT skip newlines — a newline separates
        // the assignment from any following command.
        self.lexer.skip_inline_whitespace_and_comments();
        let has_following_command = if let Some(next_token) = self.lexer.peek() {
            if !is_command_name_token(next_token) {
                false
            } else if matches!(next_token, Token::Identifier) {
                // Check whether the identifier is actually another assignment
                // (has `=`, `+=`, etc. after it) rather than a command name.
                let mut pos = 1usize;
                while matches!(
                    self.lexer.peek_n(pos),
                    Some(Token::Space | Token::Tab | Token::Comment)
                ) {
                    pos += 1;
                }
                let is_next_assignment =
                    Self::is_assignment_operator(self.lexer.peek_n(pos).cloned())
                    || self.has_indexed_assignment_after_identifier(pos);
                !is_next_assignment
            } else {
                true  // keyword or other command-starting token
            }
        } else {
            false
        };

        if has_following_command {
            // There's a command following, parse it as a command with environment variables
            let command = self.parse_command()?;

            // Merge the environment variables with the command's environment variables
            match command {
                Command::Simple(mut simple_cmd) => {
                    // Merge environment variables
                    for (key, value) in env_vars {
                        simple_cmd.env_vars.insert(key, value);
                    }
                    Ok(Command::Simple(simple_cmd))
                }
                // A redirect-wrapped simple command (`VAR=x read a <<< t` —
                // the redirect wraps the simple command): merge the env
                // into the INNER simple command — the env must scope the
                // actual command, not a sibling no-op (`VAR=x cmd <<< t`
                // with the env on a separate `true` leaves cmd without
                // the env — the IFS=, read failing case).
                Command::Redirect(redir) if matches!(&*redir.command, Command::Simple(_)) => {
                    let mut inner = match *redir.command {
                        Command::Simple(mut sc) => {
                            for (key, value) in env_vars {
                                sc.env_vars.insert(key, value);
                            }
                            Command::Simple(sc)
                        }
                        _ => unreachable!("command Simple checked"),
                    };
                    // keep the redirect's own structure
                    Ok(Command::Redirect(RedirectCommand {
                        command: Box::new(inner),
                        redirects: redir.redirects,
                    }))
                }
                _ => {
                    // For non-simple commands, wrap in a block with environment variables
                    let mut env_cmd_vars = BTreeMap::new();
                    for (key, value) in env_vars {
                        env_cmd_vars.insert(key, value);
                    }

                    let env_cmd = Command::Simple(SimpleCommand {
                        name: Word::literal("true".to_string()),
                        args: Vec::new(),
                        redirects: Vec::new(),
                        env_vars: env_cmd_vars,
                        stdout_used: true,
                        stderr_used: true,
                    });

                    Ok(Command::Block(Block {
                        commands: vec![env_cmd, command],
                    }))
                }
            }
        } else {
            // No command following, return as standalone assignment(s)
            let commands: Vec<Command> = env_vars.into_iter().map(|(variable, value)| {
                let operator = env_ops.get(&variable).cloned().unwrap_or(AssignmentOperator::Assign);
                Command::Assignment(Assignment {
                    variable,
                    value,
                    operator,
                })
            }).collect();
            if commands.len() == 1 {
                Ok(commands.into_iter().next().unwrap())
            } else {
                Ok(Command::Block(Block { commands }))
            }
        }
    }

    fn parse_subshell(&mut self) -> Result<Command, ParserError> {
        self.lexer.consume(Token::ParenOpen)?;

        // Parse one or more commands until ')'
        let mut commands = Vec::new();
        loop {
            // Skip separators within subshell body
            while matches!(
                self.lexer.peek(),
                Some(
                    Token::Space
                        | Token::Tab
                        | Token::Comment
                        | Token::Newline
                        | Token::Semicolon
                        | Token::CarriageReturn
                )
            ) {
                self.lexer.next();
            }
            match self.lexer.peek() {
                Some(Token::ParenClose) | None => break,
                _ => {
                    let mut cmd = self.parse_command()?;
                    // Background marker inside subshell
                    if let Some(Token::Background) = self.lexer.peek() {
                        self.lexer.next();
                        cmd = Command::Background(Box::new(cmd));
                    }
                    commands.push(cmd);
                }
            }
        }

        self.lexer.consume(Token::ParenClose)?;

        if commands.len() == 1 {
            Ok(Command::Subshell(Box::new(commands.remove(0))))
        } else {
            Ok(Command::Subshell(Box::new(Command::Block(Block {
                commands,
            }))))
        }
    }

    fn parse_double_paren_command(&mut self) -> Result<Command, ParserError> {
        // Parse (( ... )) arithmetic evaluation command
        // Consume the (( token
        match self.lexer.peek() {
            Some(Token::ArithmeticEval) => {
                self.lexer.next();
            }
            Some(Token::ParenOpen) => {
                self.lexer.next(); // consume first (
                if matches!(self.lexer.peek(), Some(Token::ParenOpen)) {
                    self.lexer.next(); // consume second (
                } else {
                    return Err(ParserError::InvalidSyntax(
                        "Expected (( for arithmetic evaluation".to_string(),
                    ));
                }
            }
            _ => {
                return Err(ParserError::InvalidSyntax(
                    "Expected (( for arithmetic evaluation".to_string(),
                ));
            }
        }

        // Collect the raw content inside (( ... ))
        let mut content = String::new();
        let mut paren_depth = 2; // (( contributes 2 opening parens

        loop {
            match self.lexer.peek() {
                Some(Token::ArithmeticEvalClose) => {
                    // ArithmeticEvalClose represents TWO closing parens.
                    // Only push `)` that close inner (expression) parens,
                    // not those that close the outer (( marker.
                    // Inner parens keep depth >= 2 (the 2 from (().
                    self.lexer.next();
                    let inner_count = std::cmp::max(0, paren_depth - 2);
                    paren_depth -= 2;
                    for _ in 0..inner_count {
                        content.push(')');
                    }
                    if paren_depth <= 0 {
                        break;
                    }
                }
                Some(Token::ParenOpen) => {
                    if let Some(text) = self.lexer.get_current_text() {
                        content.push_str(&text);
                    }
                    self.lexer.next();
                    paren_depth += 1;
                }
                Some(Token::ParenClose) => {
                    // Only push `)` if it closes an inner (expression) paren,
                    // not if it closes the outer (( marker.
                    // Inner parens keep depth >= 2 (the 2 from (( ).
                    paren_depth -= 1;
                    if paren_depth >= 2 {
                        if let Some(text) = self.lexer.get_current_text() {
                            content.push_str(&text);
                        }
                    }
                    self.lexer.next();
                    if paren_depth <= 0 {
                        break;
                    }
                }
                Some(Token::Arithmetic) => {
                    self.lexer.next();
                    paren_depth += 2;
                    content.push_str("$((");
                }
                Some(Token::ArithmeticEval) => {
                    self.lexer.next();
                    paren_depth += 2;
                    content.push_str("((");
                }
                Some(Token::Newline) | Some(Token::CarriageReturn) => {
                    // Skip newlines inside the arithmetic expression
                    self.lexer.next();
                }
                None => {
                    return Err(ParserError::UnexpectedEOF);
                }
                Some(Token::Comment) => {
                    // A `#` inside an arithmetic expression is the base-notation operator
                    // (e.g. 10#$x), not a comment start.  Use scan_arithmetic_comment to
                    // extract the content before `))` and inject `))` + remaining text.
                    let captured = self.lexer.scan_arithmetic_comment();
                    content.push_str(&captured);
                }
                _ => {
                    if let Some(text) = self.lexer.get_current_text() {
                        content.push_str(&text);
                    }
                    self.lexer.next();
                }
            }
        }

        let content = content.trim().to_string();

        if content.is_empty() {
            // Empty (( )) is valid in bash (sets exit code 1)
            // Generate a simple false command
            return Ok(Command::Simple(SimpleCommand {
                name: Word::Literal("false".to_string(), None),
                args: vec![],
                redirects: vec![],
                env_vars: BTreeMap::new(),
                stdout_used: true,
                stderr_used: true,
            }));
        }

        // Split by comma to get individual expressions
        let expressions = split_arithmetic_expressions(&content);

        if expressions.len() == 1 {
            let expr = expressions[0].trim();
            if let Some((var_name, value_expr)) = parse_arithmetic_assignment(expr) {
                return Ok(Command::Assignment(Assignment {
                    variable: var_name.to_string(),
                    value: Word::Arithmetic(
                        ArithmeticExpression {
                            expression: value_expr.to_string(),
                            tokens: vec![],
                        },
                        None,
                    ),
                    operator: AssignmentOperator::Assign,
                }));
            } else {
                // For non-assignment expressions like ((i++)),
                // create a simple command that evaluates the expression
                return Ok(Command::Simple(SimpleCommand {
                    name: Word::Literal("let".to_string(), None),
                    args: vec![Word::Literal(expr.to_string(), None)],
                    redirects: vec![],
                    env_vars: BTreeMap::new(),
                    stdout_used: true,
                    stderr_used: true,
                }));
            }
        }

        // Multiple comma-separated expressions
        let mut commands: Vec<Command> = Vec::new();
        for expr in &expressions {
            let expr = expr.trim();
            if expr.is_empty() {
                continue;
            }
            if let Some((var_name, value_expr)) = parse_arithmetic_assignment(expr) {
                commands.push(Command::Assignment(Assignment {
                    variable: var_name.to_string(),
                    value: Word::Arithmetic(
                        ArithmeticExpression {
                            expression: value_expr.to_string(),
                            tokens: vec![],
                        },
                        None,
                    ),
                    operator: AssignmentOperator::Assign,
                }));
            } else {
                commands.push(Command::Simple(SimpleCommand {
                    name: Word::Literal("let".to_string(), None),
                    args: vec![Word::Literal(expr.to_string(), None)],
                    redirects: vec![],
                    env_vars: BTreeMap::new(),
                    stdout_used: true,
                    stderr_used: true,
                }));
            }
        }

        if commands.is_empty() {
            return Ok(Command::Simple(SimpleCommand {
                name: Word::Literal("true".to_string(), None),
                args: vec![],
                redirects: vec![],
                env_vars: BTreeMap::new(),
                stdout_used: true,
                stderr_used: true,
            }));
        }

        if commands.len() == 1 {
            return Ok(commands.remove(0));
        }

        // Wrap multiple expressions in a block
        Ok(Command::Block(Block { commands }))
    }

    fn parse_shopt_command(&mut self) -> Result<Command, ParserError> {
        // Consume the 'shopt' token
        self.lexer.next();

        // Skip whitespace
        self.lexer.skip_whitespace_and_comments();

        // Skip any flags like -q (quiet) before -s/-u
        loop {
            match self.lexer.peek() {
                Some(Token::Minus) => {
                    // Might be -q or other flag; skip it and the following identifier
                    self.lexer.next();
                    if let Some(Token::Identifier) = self.lexer.peek() {
                        self.lexer.next();
                    }
                    self.lexer.skip_whitespace_and_comments();
                }
                _ => break,
            }
        }

        // Parse the option (e.g., -s, -u)
        let enable = if let Some(token) = self.lexer.peek() {
            match token {
                Token::Size => {
                    self.lexer.next();
                    true // -s means set (enable)
                }
                Token::SetUid => {
                    self.lexer.next();
                    false // -u means unset (disable)
                }
                _ => {
                    return Err(ParserError::InvalidSyntax(format!(
                        "Expected option after shopt, got: {:?}",
                        token
                    )));
                }
            }
        } else {
            return Err(ParserError::InvalidSyntax(
                "Expected option after shopt".to_string(),
            ));
        };

        // Skip whitespace
        self.lexer.skip_whitespace_and_comments();

        // Parse the option name (e.g., extglob, nocasematch)
        let option_name = if let Some(Token::Identifier) = self.lexer.peek() {
            let name = self.lexer.get_identifier_text()?;
            self.lexer.next();
            name
        } else {
            return Err(ParserError::InvalidSyntax(
                "Expected option name after shopt option".to_string(),
            ));
        };

        // Update the parser's shell option state
        self.update_shopt_state(&option_name, enable);

        Ok(Command::ShoptCommand(ShoptCommand {
            option: option_name, // Store the option name, not the flag
            enable,              // true for -s, false for -u
        }))
    }

    pub fn parse_test_expression(&mut self) -> Result<Command, ParserError> {
        use crate::ast::TestExpression;

        // Check if this is being called for double brackets (already consumed) or single bracket
        // If we're called from double bracket detection, the [[ tokens have already been consumed
        // If we're called for single bracket, we should see a [ token
        let is_double_bracket = !matches!(self.lexer.peek(), Some(Token::TestBracket));
        //                     eprintln!("DEBUG: parse_test_expression called, is_double_bracket: {}, current token: {:?}", is_double_bracket, self.lexer.peek());

        // If this is a double bracket test, we don't need to consume the opening brackets
        // If this is a single bracket test, we need to consume the opening [
        if !is_double_bracket {
            self.lexer.next(); // consume the [
        }

        // Capture the content between brackets
        let mut expression_parts = Vec::new();

        //         eprintln!("DEBUG: Starting to capture expression content, current token: {:?}", self.lexer.peek());

        loop {
            let current_token = self.lexer.peek();
            match current_token {
                Some(Token::TestBracketClose) => {
                    if is_double_bracket {
                        // For [[ ]], we need to consume two closing brackets
                        self.lexer.next(); // consume first ']'
                        if matches!(self.lexer.peek(), Some(Token::TestBracketClose)) {
                            self.lexer.next(); // consume second ']'
                            break;
                        } else {
                            // Add the first ] to the expression and continue
                            expression_parts.push("]".to_string());
                        }
                    } else {
                        // For [ ], consume one closing bracket
                        self.lexer.next(); // consume ']'
                        break;
                    }
                }
                Some(Token::File) => {
                    expression_parts.push("-f ".to_string());
                    self.lexer.next();
                }
                Some(Token::Directory) => {
                    expression_parts.push("-d ".to_string());
                    self.lexer.next();
                }
                Some(Token::Exists) => {
                    expression_parts.push("-e ".to_string());
                    self.lexer.next();
                }
                Some(Token::Readable) => {
                    expression_parts.push("-r ".to_string());
                    self.lexer.next();
                }
                Some(Token::Writable) => {
                    expression_parts.push("-w ".to_string());
                    self.lexer.next();
                }
                Some(Token::Executable) => {
                    expression_parts.push("-x ".to_string());
                    self.lexer.next();
                }
                Some(Token::Size) => {
                    expression_parts.push("-s ".to_string());
                    self.lexer.next();
                }
                Some(Token::Symlink) => {
                    expression_parts.push("-L ".to_string());
                    self.lexer.next();
                }
                Some(Token::Equality) => {
                    expression_parts.push("==".to_string());
                    self.lexer.next();
                }
                Some(Token::RegexMatch) => {
                    expression_parts.push("=~".to_string());
                    self.lexer.next();
                }
                Some(Token::Star) => {
                    expression_parts.push("*".to_string());
                    self.lexer.next();
                }
                Some(Token::Dot) => {
                    expression_parts.push(".".to_string());
                    self.lexer.next();
                }
                Some(Token::Bang) => {
                    expression_parts.push("!".to_string());
                    self.lexer.next();
                }
                Some(Token::ParenOpen) => {
                    expression_parts.push("(".to_string());
                    self.lexer.next();
                }
                Some(Token::ParenClose) => {
                    expression_parts.push(")".to_string());
                    self.lexer.next();
                }
                Some(Token::TestBracket) => {
                    // Read the full [...] expression
                    expression_parts.push("[".to_string());
                    self.lexer.next();
                    // Collect content until TestBracketClose
                    loop {
                        match self.lexer.peek() {
                            Some(Token::TestBracketClose) => {
                                expression_parts.push("]".to_string());
                                self.lexer.next();
                                break;
                            }
                            Some(Token::Space) | Some(Token::Tab) => {
                                expression_parts.push(" ".to_string());
                                self.lexer.next();
                            }
                            _ => {
                                if let Some(text) = self.lexer.get_current_text() {
                                    expression_parts.push(text);
                                }
                                self.lexer.next();
                            }
                        }
                    }
                }
                Some(Token::Caret) => {
                    expression_parts.push("^".to_string());
                    self.lexer.next();
                }
                Some(Token::Plus) => {
                    expression_parts.push("+".to_string());
                    self.lexer.next();
                }
                Some(Token::Escape) | Some(Token::EscapedDoubleQuote) | Some(Token::EscapedSingleQuote) | Some(Token::EscapedBacktick) => {
                    expression_parts.push("\\".to_string());
                    self.lexer.next();
                }
                Some(Token::DollarHashSimple) => {
                    expression_parts.push("$#".to_string());
                    self.lexer.next();
                }
                Some(Token::DollarAtSimple) => {
                    expression_parts.push("$@".to_string());
                    self.lexer.next();
                }
                Some(Token::DollarStarSimple) => {
                    expression_parts.push("$*".to_string());
                    self.lexer.next();
                }
                Some(Token::Dollar) => {
                    // Handle variable reference: $variable or regex anchor: $
                    if let Some(Token::Identifier) = self.lexer.peek_n(1) {
                        self.lexer.next(); // consume the $
                        let identifier = self.lexer.get_identifier_text()?;
                        expression_parts.push(format!("${}", identifier));
                    } else {
                        expression_parts.push("$".to_string());
                        self.lexer.next();
                    }
                }
                Some(Token::DollarBrace)
                | Some(Token::DollarBraceHash)
                | Some(Token::DollarBraceBang)
                | Some(Token::DollarBraceStar)
                | Some(Token::DollarBraceAt)
                | Some(Token::DollarBraceHashStar)
                | Some(Token::DollarBraceHashAt)
                | Some(Token::DollarBraceBangStar)
                | Some(Token::DollarBraceBangAt) => {
                    let mut expansion = self.lexer.get_raw_token_text()?;
                    let mut brace_depth = 1usize;
                    while brace_depth > 0 {
                        match self.lexer.peek() {
                            Some(Token::BraceClose) => {
                                expansion.push_str(&self.lexer.get_raw_token_text()?);
                                brace_depth -= 1;
                            }
                            Some(Token::DollarBrace)
                            | Some(Token::DollarBraceHash)
                            | Some(Token::DollarBraceBang)
                            | Some(Token::DollarBraceStar)
                            | Some(Token::DollarBraceAt)
                            | Some(Token::DollarBraceHashStar)
                            | Some(Token::DollarBraceHashAt)
                            | Some(Token::DollarBraceBangStar)
                            | Some(Token::DollarBraceBangAt) => {
                                expansion.push_str(&self.lexer.get_raw_token_text()?);
                                brace_depth += 1;
                            }
                            Some(Token::Comment) => {
                                // A `#` inside ${...} is a parameter-expansion operator
                                // (${var#pattern}, ${var##pattern}), not a comment start.
                                // We need to split the Comment at `}` and inject any
                                // text after `}` as re-lexed tokens (e.g. `]; then`).
                                let text = self.lexer.handle_comment_with_brace(brace_depth)?;
                                expansion.push_str(&text);
                                brace_depth = 0;
                                break;
                            }
                            Some(_) => expansion.push_str(&self.lexer.get_raw_token_text()?),
                            None => {
                                return Err(ParserError::InvalidSyntax(
                                    "Unexpected end of input in parameter expansion".to_string(),
                                ))
                            }
                        }
                    }
                    expression_parts.push(expansion);
                }
                Some(Token::DoubleQuotedString) | Some(Token::SingleQuotedString) => {
                    let string_text = self.lexer.get_string_text()?;
                    expression_parts.push(string_text);
                }
                Some(Token::Space) | Some(Token::Tab) => {
                    self.lexer.next(); // skip whitespace
                }
                Some(Token::Identifier) => {
                    let identifier = self.lexer.get_identifier_text()?;
                    expression_parts.push(identifier);
                }
                Some(Token::RegexPattern) => {
                    let pattern_text = self.lexer.get_raw_token_text()?;
                    expression_parts.push(pattern_text);
                }
                Some(Token::Tilde) => {
                    // Handle tilde expansion: ~ or ~/path
                    expression_parts.push("~".to_string());
                    self.lexer.next();
                }
                Some(Token::Slash) => {
                    // Handle path separators after tilde
                    expression_parts.push("/".to_string());
                    self.lexer.next();
                }
                Some(Token::Assign) => {
                    // Handle assignment operator in test expressions
                    expression_parts.push("=".to_string());
                    self.lexer.next();
                }
                Some(Token::Lt) => {
                    expression_parts.push(" -lt ".to_string());
                    self.lexer.next();
                }
                Some(Token::Le) => {
                    expression_parts.push(" -le ".to_string());
                    self.lexer.next();
                }
                Some(Token::Gt) => {
                    expression_parts.push(" -gt ".to_string());
                    self.lexer.next();
                }
                Some(Token::Ge) => {
                    expression_parts.push(" -ge ".to_string());
                    self.lexer.next();
                }
                Some(Token::Eq) => {
                    expression_parts.push(" -eq ".to_string());
                    self.lexer.next();
                }
                Some(Token::Ne) => {
                    expression_parts.push(" -ne ".to_string());
                    self.lexer.next();
                }
                Some(Token::Number) | Some(Token::Float) | Some(Token::PaddedNumber) | Some(Token::HexNumber) => {
                    let num = self.lexer.get_raw_token_text()?;
                    expression_parts.push(num);
                }
                Some(Token::NonZero) => {
                    expression_parts.push(" -n ".to_string());
                    self.lexer.next();
                }
                Some(Token::Zero) => {
                    expression_parts.push(" -z ".to_string());
                    self.lexer.next();
                }
                Some(Token::And) => {
                    expression_parts.push(" -a ".to_string());
                    self.lexer.next();
                }
                Some(Token::Or) => {
                    expression_parts.push(" -o ".to_string());
                    self.lexer.next();
                }
                Some(Token::Arithmetic) | Some(Token::ArithmeticEval) => {
                    // Handle $(( expr )) or (( expr )) arithmetic inside test expression
                    self.lexer.next(); // consume $(( or ((
                    let mut arith = String::new();
                    let mut depth = 2usize; // (( or $(( contributes 2 opening parens
                    loop {
                        match self.lexer.peek() {
                            Some(Token::ArithmeticEvalClose) => {
                                // ArithmeticEvalClose represents TWO closing parens
                                self.lexer.next();
                                depth -= 2;
                                if depth <= 0 {
                                    break;
                                }
                                arith.push_str("))");
                            }
                            Some(Token::ParenOpen) => {
                                if let Some(text) = self.lexer.get_current_text() {
                                    arith.push_str(&text);
                                }
                                self.lexer.next();
                                depth += 1;
                            }
                            Some(Token::ParenClose) => {
                                if let Some(text) = self.lexer.get_current_text() {
                                    arith.push_str(&text);
                                }
                                self.lexer.next();
                                depth -= 1;
                                if depth <= 0 {
                                    break;
                                }
                            }
                            Some(Token::Arithmetic) | Some(Token::ArithmeticEval) => {
                                self.lexer.next();
                                depth += 2;
                                arith.push_str("$((");
                            }
                            None => break,
                            Some(Token::Comment) => {
                                // A `#` inside an arithmetic expression is the base-notation
                                // operator (e.g. 10#$x), not a comment start.
                                let captured = self.lexer.scan_arithmetic_comment();
                                arith.push_str(&captured);
                            }
                            _ => {
                                if let Some(text) = self.lexer.get_current_text() {
                                    arith.push_str(&text);
                                }
                                self.lexer.next();
                            }
                        }
                    }
                    expression_parts.push(format!("$(({}))", arith));
                }
                Some(Token::Semicolon) => {
                    // Should not appear inside test expression, treat as end
                    break;
                }
                Some(Token::Newline) | Some(Token::CarriageReturn) => {
                    // In double-bracket [[ ]], newlines are whitespace (expressions can span lines)
                    // In single-bracket [ ], newlines end the expression
                    if is_double_bracket {
                        self.lexer.next(); // skip newline
                    } else {
                        break;
                    }
                }
                Some(Token::Minus) => {
                    // POSIX test -a (AND) / -o (OR) operators are lexed as Minus + Identifier
                    self.lexer.next(); // consume -
                    match self.lexer.peek() {
                        Some(Token::Identifier) => {
                            let id = self.lexer.get_identifier_text()?;
                            match id.as_str() {
                                "a" => expression_parts.push(" -a ".to_string()),
                                "o" => expression_parts.push(" -o ".to_string()),
                                other => expression_parts.push(format!("-{}", other)),
                            }
                        }
                        _ => expression_parts.push("-".to_string()),
                    }
                }
                // Handle redirect tokens inside test expressions as literal characters
                // (e.g., `\>` for string comparison in `[ ]`)
                Some(Token::RedirectIn) | Some(Token::RedirectOut) | Some(Token::RedirectAppend)
                | Some(Token::RedirectInOut) | Some(Token::RedirectAll)
                | Some(Token::RedirectAllAppend) | Some(Token::RedirectInErr)
                | Some(Token::RedirectOutErr) | Some(Token::RedirectOutClobber) => {
                    let text = self.lexer.get_raw_token_text().unwrap_or_default();
                    expression_parts.push(text);
                }
                // Handle missing test operator tokens
                Some(Token::Socket) => { expression_parts.push(" -S ".to_string()); self.lexer.next(); }
                Some(Token::SymlinkH) => { expression_parts.push(" -h ".to_string()); self.lexer.next(); }
                Some(Token::PipeFile) => { expression_parts.push(" -p ".to_string()); self.lexer.next(); }
                Some(Token::Block) => { expression_parts.push(" -b ".to_string()); self.lexer.next(); }
                Some(Token::Character) => { expression_parts.push(" -c ".to_string()); self.lexer.next(); }
                Some(Token::SetGid) => { expression_parts.push(" -g ".to_string()); self.lexer.next(); }
                Some(Token::Sticky) => { expression_parts.push(" -k ".to_string()); self.lexer.next(); }
                Some(Token::SetUid) => { expression_parts.push(" -u ".to_string()); self.lexer.next(); }
                Some(Token::Owned) => { expression_parts.push(" -O ".to_string()); self.lexer.next(); }
                Some(Token::GroupOwned) => { expression_parts.push(" -G ".to_string()); self.lexer.next(); }
                Some(Token::Modified) => { expression_parts.push(" -N ".to_string()); self.lexer.next(); }
                Some(Token::NewerThan) => { expression_parts.push(" -nt ".to_string()); self.lexer.next(); }
                Some(Token::OlderThan) => { expression_parts.push(" -ot ".to_string()); self.lexer.next(); }
                Some(Token::SameFile) => { expression_parts.push(" -ef ".to_string()); self.lexer.next(); }
                Some(Token::At) => { expression_parts.push("@".to_string()); self.lexer.next(); }
                Some(Token::Colon) => { expression_parts.push(":".to_string()); self.lexer.next(); }
                Some(Token::Pipe) => { expression_parts.push("|".to_string()); self.lexer.next(); }
                Some(Token::BraceOpen) => { expression_parts.push("{".to_string()); self.lexer.next(); }
                Some(Token::BraceClose) => { expression_parts.push("}".to_string()); self.lexer.next(); }
                Some(Token::Comma) => { expression_parts.push(",".to_string()); self.lexer.next(); }
                Some(Token::Percent) => { expression_parts.push("%".to_string()); self.lexer.next(); }
                Some(Token::Question) => { expression_parts.push("?".to_string()); self.lexer.next(); }
                Some(Token::Background) => { expression_parts.push("&".to_string()); self.lexer.next(); }
                Some(Token::PlusAssign) | Some(Token::MinusAssign) | Some(Token::StarAssign) | Some(Token::SlashAssign) | Some(Token::PercentAssign) => {
                    let text = self.lexer.get_raw_token_text().unwrap_or_default();
                    expression_parts.push(text);
                }
                Some(Token::DollarParen) => {
                    // Handle $() command substitution inside test expressions.
                    // Track nested parens so we don't stop at the first )
                    // that closes an inner subshell rather than the $().
                    let mut sub = "$(".to_string();
                    self.lexer.next(); // consume $(
                    let mut depth = 1usize;
                    loop {
                        match self.lexer.peek() {
                            Some(Token::DollarParen) => {
                                sub.push_str(&self.lexer.get_raw_token_text()?);
                                depth += 1;
                            }
                            Some(Token::ParenOpen) => {
                                sub.push('(');
                                self.lexer.next();
                                depth += 1;
                            }
                            Some(Token::ParenClose) => {
                                sub.push(')');
                                self.lexer.next();
                                depth -= 1;
                                if depth == 0 { break; }
                            }
                            Some(Token::ArithmeticEvalClose) => {
                                // )) closes two levels of paren depth
                                depth = depth.saturating_sub(2);
                                sub.push_str("))");
                                self.lexer.next();
                                if depth == 0 { break; }
                            }
                            Some(_) => {
                                sub.push_str(&self.lexer.get_raw_token_text()?);
                            }
                            None => {
                                // If we run out of tokens, use whatever we have
                                break;
                            },
                        }
                    }
                    expression_parts.push(sub);
                }
                Some(Token::BacktickString) => {
                    let text = self.lexer.get_raw_token_text()?;
                    expression_parts.push(text);
                }
                Some(Token::LongOption) => {
                    let text = self.lexer.get_raw_token_text()?;
                    expression_parts.push(text);
                }
                Some(Token::True) => {
                    expression_parts.push("true".to_string());
                    self.lexer.next();
                }
                Some(Token::False) => {
                    expression_parts.push("false".to_string());
                    self.lexer.next();
                }
                Some(Token::DollarQuestion) => {
                    expression_parts.push("$?".to_string());
                    self.lexer.next();
                }
                Some(Token::DollarDollar) => {
                    expression_parts.push("$$".to_string());
                    self.lexer.next();
                }
                Some(Token::DollarBang) => {
                    expression_parts.push("$!".to_string());
                    self.lexer.next();
                }
                Some(Token::DollarMinus) => {
                    expression_parts.push("$-".to_string());
                    self.lexer.next();
                }
                None => {
                    return Err(ParserError::InvalidSyntax(
                        "Unexpected end of input in test expression".to_string(),
                    ));
                }
                _ => {
                    // In a test expression, shell keywords and many other tokens
                    // can appear as literal values (e.g. `set` in `[ x = set ]`).
                    // Treat them as literal text rather than failing.
                    if let Some(text) = self.lexer.get_current_text() {
                        expression_parts.push(text);
                        self.lexer.next();
                    } else {
                        let token_str = format!("{:?}", self.lexer.peek());
                        return Err(ParserError::InvalidSyntax(format!(
                            "Unexpected token in test expression: {}",
                            token_str
                        )));
                    }
                }
            }
        }

        let expression = expression_parts.join("");

        let mut modifiers = self.get_current_shopt_state();
        // `[[ ]]` (double-bracket — the caller consumed the `[[` already)
        // vs `[ ]` (single): the A1 test Call carries the style as a
        // trailing tag arg (core request extglob-nocasematch-20260806).
        modifiers.double = is_double_bracket;
        Ok(Command::TestExpression(TestExpression {
            expression,
            modifiers,
        }))
    }

    fn parse_variable_expansion(&mut self) -> Result<Word, ParserError> {
        // Check what type of variable expansion we have
        match self.lexer.peek() {
            Some(Token::Dollar) => {
                // Simple variable reference like $i
                self.lexer.next(); // consume the $ token

                // Expect an identifier after the $
                if let Some(Token::Identifier) = self.lexer.peek() {
                    let var_name = self.lexer.get_identifier_text()?;
                    Ok(Word::variable(var_name))
                } else {
                    Err(ParserError::InvalidSyntax(
                        "Expected identifier after $ in variable expansion".to_string(),
                    ))
                }
            }
            Some(Token::DollarBrace) => {
                // Parameter expansion like ${i}
                self.lexer.next(); // consume the ${ token

                // Parse the content until we find the closing }
                let mut expression_parts = Vec::new();

                loop {
                    match self.lexer.peek() {
                        Some(Token::BraceClose) => {
                            // Found the closing }, consume it and break
                            self.lexer.next();
                            break;
                        }
                        Some(Token::Identifier) => {
                            // Variable name like 'i'
                            let var_name = self.lexer.get_identifier_text()?;
                            expression_parts.push(var_name);
                            // Note: get_identifier_text already advances, no extra next() needed
                        }
                        Some(Token::Number) => {
                            // Number like '1'
                            let num_text = self.lexer.get_number_text()?;
                            expression_parts.push(num_text);
                            // Note: get_number_text already advances, no extra next() needed
                        }
                        Some(Token::Space) | Some(Token::Tab) => {
                            // Skip whitespace
                            self.lexer.next();
                        }
                        None => {
                            return Err(ParserError::InvalidSyntax(
                                "Unexpected end of input in parameter expansion".to_string(),
                            ));
                        }
                        Some(Token::Comment) => {
                            // A `#` inside ${...} is a parameter-expansion operator
                            // (${var#pattern}), not a comment start.
                            let text = self.lexer.get_raw_token_text()?;
                            if let Some(pos) = text.find('}') {
                                expression_parts.push(text[..pos].to_string());
                                break;
                            } else {
                                expression_parts.push(text);
                            }
                        }
                        _ => {
                            // In parameter expansion, operators like %, #, :, -, +, ?, /, = are valid
                            // Also handle Star for glob patterns like ${var%pattern}
                            let raw_text = self.lexer.get_raw_token_text()?;
                            expression_parts.push(raw_text);
                        }
                    }
                }

                // For now, just create a simple parameter expansion
                // In a full implementation, this would parse operators like :-, :+, :?, etc.
                let var_name = expression_parts.join("");
                Ok(Word::parameter_expansion(ParameterExpansion {
                    variable: var_name,
                    operator: ParameterExpansionOperator::None,
                    is_mutable: true,
                }))
            }
            _ => Err(ParserError::InvalidSyntax(
                "Expected $ or ${ in variable expansion".to_string(),
            )),
        }
    }

    fn parse_arithmetic_expression(&mut self) -> Result<Word, ParserError> {
        // Handle arithmetic expressions like $((i + 1))
        // The lexer should have already consumed the opening $( tokens
        // We need to parse the content until we find the closing ))

        let mut expression_parts = Vec::new();

        loop {
            match self.lexer.peek() {
                Some(Token::ArithmeticEvalClose) => {
                    // Found the closing )), consume it and break
                    self.lexer.next();
                    break;
                }
                Some(Token::Identifier) => {
                    // Variable reference like 'i'
                    let var_name = self.lexer.get_identifier_text()?;
                    expression_parts.push(var_name);
                    self.lexer.next(); // consume the identifier token
                }
                Some(Token::Number) => {
                    // Number like '1'
                    let num_text = self.lexer.get_number_text()?;
                    expression_parts.push(num_text);
                    self.lexer.next(); // consume the number token
                }
                Some(Token::Plus) => {
                    // Plus operator
                    self.lexer.next();
                    expression_parts.push("+".to_string());
                }
                Some(Token::Minus) => {
                    // Minus operator
                    self.lexer.next();
                    expression_parts.push("-".to_string());
                }
                Some(Token::Star) => {
                    // Multiplication operator
                    self.lexer.next();
                    expression_parts.push("*".to_string());
                }
                Some(Token::Slash) => {
                    // Division operator
                    self.lexer.next();
                    expression_parts.push("/".to_string());
                }
                Some(Token::Percent) => {
                    // Modulo operator
                    self.lexer.next();
                    expression_parts.push("%".to_string());
                }
                Some(Token::Equality) => {
                    // Equality comparison
                    self.lexer.next();
                    expression_parts.push("==".to_string());
                }
                Some(Token::Dollar) => {
                    // Variable reference inside arithmetic
                    self.lexer.next();
                    if let Some(Token::Identifier) = self.lexer.peek() {
                        let var_name = self.lexer.get_identifier_text()?;
                        expression_parts.push(format!("${}", var_name));
                    } else if let Some(Token::Arithmetic) = self.lexer.peek() {
                        // Nested $((...)) arithmetic
                        let text = self.lexer.get_raw_token_text()?;
                        let mut depth = 1usize;
                        expression_parts.push(text);
                        while depth > 0 {
                            match self.lexer.peek() {
                                Some(Token::ArithmeticEvalClose) => {
                                    expression_parts.push(self.lexer.get_raw_token_text()?);
                                    depth -= 1;
                                }
                                Some(Token::Arithmetic) => {
                                    expression_parts.push(self.lexer.get_raw_token_text()?);
                                    depth += 1;
                                }
                                Some(_) => {
                                    expression_parts.push(self.lexer.get_raw_token_text()?);
                                }
                                None => break,
                            }
                        }
                    } else {
                        expression_parts.push("$".to_string());
                    }
                }
                Some(Token::DollarParen) => {
                    // $() command substitution inside arithmetic
                    let text = self.lexer.get_raw_token_text()?;
                    let mut depth = 1usize;
                    expression_parts.push(text);
                    while depth > 0 {
                        match self.lexer.peek() {
                            Some(Token::ParenClose) => {
                                expression_parts.push(self.lexer.get_raw_token_text()?);
                                depth -= 1;
                            }
                            Some(Token::DollarParen) => {
                                expression_parts.push(self.lexer.get_raw_token_text()?);
                                depth += 1;
                            }
                            Some(Token::ArithmeticEvalClose) => {
                                // )) closes two levels of paren depth
                                depth = depth.saturating_sub(2);
                                expression_parts.push(self.lexer.get_raw_token_text()?);
                            }
                            Some(_) => {
                                expression_parts.push(self.lexer.get_raw_token_text()?);
                            }
                            None => break,
                        }
                    }
                }
                Some(Token::Arithmetic) => {
                    // Nested $((...)) arithmetic
                    let text = self.lexer.get_raw_token_text()?;
                    let mut depth = 1usize;
                    expression_parts.push(text);
                    while depth > 0 {
                        match self.lexer.peek() {
                            Some(Token::ArithmeticEvalClose) => {
                                expression_parts.push(self.lexer.get_raw_token_text()?);
                                depth -= 1;
                            }
                            Some(Token::Arithmetic) => {
                                expression_parts.push(self.lexer.get_raw_token_text()?);
                                depth += 1;
                            }
                            Some(_) => {
                                expression_parts.push(self.lexer.get_raw_token_text()?);
                            }
                            None => break,
                        }
                    }
                }
                Some(Token::ParenOpen) => {
                    self.lexer.next();
                    expression_parts.push("(".to_string());
                }
                Some(Token::ParenClose) => {
                    self.lexer.next();
                    expression_parts.push(")".to_string());
                }
                Some(Token::Space) | Some(Token::Tab) => {
                    // Skip whitespace
                    self.lexer.next();
                }
                None => {
                    return Err(ParserError::InvalidSyntax(
                        "Unexpected end of input in arithmetic expression".to_string(),
                    ));
                }
                _ => {
                    return Err(ParserError::InvalidSyntax(
                        "Unexpected token in arithmetic expression".to_string(),
                    ));
                }
            }
        }

        // Create an arithmetic expression word
        let expression = expression_parts.join("");
        Ok(Word::arithmetic(ArithmeticExpression {
            expression,
            tokens: vec![], // For now, leave tokens empty
        }))
    }

    fn update_shopt_state(&mut self, option: &str, enable: bool) {
        match option {
            "extglob" => self.shopt_state.extglob = enable,
            "nocasematch" => self.shopt_state.nocasematch = enable,
            "globstar" => self.shopt_state.globstar = enable,
            "nullglob" => self.shopt_state.nullglob = enable,
            "failglob" => self.shopt_state.failglob = enable,
            "dotglob" => self.shopt_state.dotglob = enable,
            _ => {} // Ignore unknown options
        }
    }

    fn get_current_shopt_state(&self) -> TestModifiers {
        self.shopt_state.to_owned()
    }
}

fn is_builtin_command(name: &str) -> bool {
    matches!(
        name,
        "set"
            | "unset"
            | "export"
            | "readonly"
            | "declare"
            | "typeset"
            | "local"
            | "shift"
            | "eval"
            | "exec"
            | "source"
            | "trap"
            | "wait"
            | "shopt"
            | "exit"
            | "return"
            | "break"
            | "continue"
    )
}

// Helper function to parse a pipeline from text
/// Split arithmetic expression by commas, respecting parentheses.
fn split_arithmetic_expressions(content: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut current = String::new();
    for ch in content.chars() {
        match ch {
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                parts.push(current.trim().to_string());
                current = String::new();
            }
            _ => {
                current.push(ch);
            }
        }
    }
    let remaining = current.trim().to_string();
    if !remaining.is_empty() {
        parts.push(remaining);
    }
    parts
}

/// Parse an arithmetic assignment expression like "i = 1 + 2" into variable name and value.
/// Returns None if the expression is not a simple assignment.
fn parse_arithmetic_assignment<'a>(expr: &'a str) -> Option<(&'a str, &'a str)> {
    // Find the first `=` that is not part of ==, !=, <=, >=, +=, -=, *=, /=, %=
    let bytes = expr.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' {
            // Check if this is a compound operator
            if i > 0 {
                let prev = bytes[i - 1];
                if prev == b'<' || prev == b'>' || prev == b'!' || prev == b'+' || prev == b'-' || prev == b'*' || prev == b'/' || prev == b'%' {
                    i += 1;
                    continue;
                }
            }
            // Check for ==
            if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                i += 2;
                continue;
            }
            // Found assignment =
            let var_name = expr[..i].trim();
            let value_expr = expr[i + 1..].trim();
            if !var_name.is_empty() && !value_expr.is_empty() {
                return Some((var_name, value_expr));
            }
            return None;
        }
        i += 1;
    }
    None
}

/// Parse text as a pipeline, reporting whether the ENTIRE text was consumed.
/// The plain pipeline parser stops at the first command separator and
/// silently drops any trailing commands; command-substitution bodies with
/// multiple commands (`$(cmd1\ncmd2)`) must detect that and reparse with
/// the full parser instead.
pub fn parse_pipeline_from_text_with_rest(
    text: &str,
) -> Result<(Command, bool), ParserError> {
    use crate::lexer::{Lexer, Token};

    let mut lexer = Lexer::new(text);
    let mut parser = Parser::new_with_lexer(lexer);
    let cmd = parser.parse_pipeline()?;
    // Skip trailing separators/whitespace, then report what remains.
    while let Some(tok) = parser.lexer.peek() {
        match tok {
            Token::Space | Token::Tab | Token::Newline | Token::CarriageReturn
            | Token::Semicolon | Token::Comment => {
                parser.lexer.next();
            }
            _ => break,
        }
    }
    Ok((cmd, parser.lexer.is_eof()))
}

pub fn parse_pipeline_from_text(text: &str) -> Result<Command, ParserError> {
    use crate::lexer::{Lexer, Token};

    // Create a lexer for the command text
    let mut lexer = Lexer::new(text);

    // Create a parser with the lexer
    let mut parser = Parser::new_with_lexer(lexer);

    // Parse as a pipeline
    parser.parse_pipeline()
}

/// Parse text as one or more commands, using the full parser that handles
/// compound constructs (for, if, while, case, etc.) as well as pipelines.
pub fn parse_commands_from_text(text: &str) -> Result<Vec<Command>, ParserError> {
    let mut parser = Parser::new(text);
    parser.parse()
}

// Re-export the main parsing function
pub fn parse(input: &str) -> Result<Vec<Command>, ParserError> {
    let mut parser = Parser::new(input);
    parser.parse()
}
