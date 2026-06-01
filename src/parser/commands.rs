use crate::ast::*;
use crate::lexer::{Lexer, Token};
use crate::parser::assignments::parse_array_elements;
use crate::parser::control_flow::{
    parse_block, parse_break_statement, parse_case_statement, parse_continue_statement,
    parse_for_loop, parse_function, parse_if_statement, parse_posix_function,
    parse_return_statement, parse_while_loop,
};
use crate::parser::errors::ParserError;
use crate::parser::redirects::parse_redirect;
use crate::parser::utilities::ParserUtilities;
use crate::parser::words::{parse_word, parse_word_no_newline_skip};
use std::collections::HashMap;

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
                Some(Token::Newline) => {
                    newline_count += 1;
                    self.lexer.next();
                }
                _ => break,
            }
        }

        while !self.lexer.is_eof() {
            let _current_token = self.lexer.peek();

            if self.lexer.is_eof() {
                break;
            }

            // Check if we're at a newline before parsing the command
            if let Some(Token::Newline) = self.lexer.peek() {
                // Consume the newline and continue to next iteration
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
            // Skip whitespace and comments
            self.lexer.skip_whitespace_and_comments();

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
                    Some(Token::Newline) => {
                        newline_count += 1;
                        self.lexer.next();
                    }
                    Some(Token::Semicolon) => {
                        self.lexer.next();
                        break;
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
            let paren1 = self.lexer.peek_n(1);
            let paren2 = self.lexer.peek_n(2);
            if matches!(paren1, Some(Token::ParenOpen)) && matches!(paren2, Some(Token::ParenClose))
            {
                // Check if the next non-whitespace token is a brace
                let mut pos = 3;
                while pos < 10
                    && matches!(
                        self.lexer.peek_n(pos),
                        Some(Token::Space | Token::Tab | Token::Comment | Token::Newline)
                    )
                {
                    pos += 1;
                }
                let brace_token = self.lexer.peek_n(pos);
                if matches!(brace_token, Some(Token::BraceOpen)) {
                    parse_posix_function(self)?
                } else {
                    self.parse_pipeline()?
                }
            } else {
                // Check if this is a standalone variable assignment: identifier=value or identifier[subscript]=value
                let mut pos = 1;
                while pos < 10
                    && matches!(
                        self.lexer.peek_n(pos),
                        Some(Token::Space | Token::Tab | Token::Comment | Token::Newline)
                    )
                {
                    pos += 1;
                }

                // Check for simple assignment: identifier=value
                if Self::is_assignment_operator(self.lexer.peek_n(pos).cloned())
                    || self.has_indexed_assignment_after_identifier(1)
                {
                    self.parse_standalone_assignment()?
                } else {
                    self.parse_pipeline()?
                }
            }
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
                            env_vars: HashMap::new(),
                            stdout_used: true,
                            stderr_used: true,
                        })),
                        redirects,
                    })
                }
                // Bash arithmetic evaluation: (( ... ))
                Some(Token::ParenOpen)
                    if matches!(self.lexer.peek_n(1), Some(Token::ParenOpen)) =>
                {
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
                Some(Token::Semicolon) => {
                    // Skip semicolon and continue parsing
                    self.lexer.next();
                    self.parse_command()?
                }
                Some(Token::Newline) | Some(Token::CarriageReturn) => {
                    // Newlines should be handled at the top level, not here
                    // Return an empty command to indicate we hit a newline
                    return Ok(Command::Simple(SimpleCommand {
                        name: Word::literal("".to_string()),
                        args: vec![],
                        redirects: vec![],
                        env_vars: HashMap::new(),
                        stdout_used: true,
                        stderr_used: true,
                    }));
                }
                _ => self.parse_pipeline()?,
            }
        };

        let command = self.parse_command_redirects(command)?;

        self.lexer.skip_whitespace_and_comments();
        if matches!(
            self.lexer.peek(),
            Some(Token::And | Token::Or | Token::Pipe)
        ) {
            self.parse_pipeline_from_command(command, 0)
        } else {
            Ok(command)
        }
    }

    fn parse_command_redirects(&mut self, command: Command) -> Result<Command, ParserError> {
        // Check if there are redirects following the command
        let mut redirects = Vec::new();

        // Skip whitespace and comments
        self.lexer.skip_whitespace_and_comments();

        // Parse redirects until we hit a command separator or other non-redirect token
        while let Some(token) = self.lexer.peek() {
            match token {
                Token::Space => {
                    // Skip whitespace between consecutive redirects (e.g. the space
                    // between `<(sort a.txt)` and `<(sort b.txt)` in
                    // `comm -23 <(sort a.txt) <(sort b.txt)`).
                    self.lexer.skip_whitespace_and_comments();
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
                }
                _ => break,
            }
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
        if matches!(self.lexer.peek_n(start_pos), Some(Token::CasePattern)) {
            let mut pos = start_pos + 1;
            while pos < start_pos + 16
                && matches!(
                    self.lexer.peek_n(pos),
                    Some(Token::Space | Token::Tab | Token::Comment | Token::Newline)
                )
            {
                pos += 1;
            }
            return Self::is_assignment_operator(self.lexer.peek_n(pos).cloned());
        }

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
            Some(Token::CasePattern) => {
                var_name.push_str(&self.lexer.get_raw_token_text()?);
            }
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
                    suffix.push('[');
                    self.lexer.next();
                    depth += 1;
                }
                Some(Token::TestBracketClose) => {
                    suffix.push(']');
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
            Some(Token::For) => parse_for_loop(self),
            Some(Token::Function) => parse_function(self),
            Some(Token::ParenOpen) if matches!(self.lexer.peek_n(1), Some(Token::ParenOpen)) => {
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
            // Skip any whitespace/comments before checking for an operator
            self.lexer.skip_whitespace_and_comments();
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

                    // Build the left side for this operator:
                    // - If pipe_commands is non-empty, flush it (and optionally wrap
                    //   the previously accumulated result around it).
                    // - If pipe_commands is empty we had a previous `&&`/`||`, so
                    //   the accumulated result IS the left side.
                    let left = if pipe_commands.is_empty() {
                        // result must be Some — the previous `&&`/`||` stored it
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
                    // further `&&`/`||` — those are handled by the outer loop to
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
        let mut env_vars = HashMap::new();

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
                                Some(_) => pos += 1,
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
                    } else if matches!(self.lexer.peek_n(1), Some(Token::CasePattern)) {
                        self.lexer.peek_n(2).cloned()
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
                            self.lexer.skip_whitespace_and_comments();
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
                            self.lexer.skip_whitespace_and_comments();
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
                                                // For other types, use parse_word
                                                parse_word(&mut self.lexer)?
                                            }
                                        };

                                        // Create assignment word: var=value
                                        // Handle command substitutions properly
                                        let assignment_word = match &value_word {
                                            Word::CommandSubstitution(_cmd, _) => {
                                                // For command substitutions, create a proper assignment
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

                                        // If the value is a command substitution, add it as a separate argument
                                        if let Word::CommandSubstitution(cmd, _) = value_word {
                                            args.push(Word::CommandSubstitution(cmd, None));
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
                            | Token::Background => break,
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
                | Token::Escape => {
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
            )
            | None
        ) {
            // Empty value (e.g. IFS= read ...)
            Word::literal(String::new())
        } else {
            parse_word(&mut self.lexer)?
        };

        // Check if there's a command following this assignment
        self.lexer.skip_whitespace_and_comments();
        if let Some(Token::Identifier) = self.lexer.peek() {
            // There's a command following, parse it as a command with environment variables
            let mut env_vars = HashMap::new();
            env_vars.insert(var_name, value_word);

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
                _ => {
                    // For non-simple commands, wrap in a block with environment variables
                    let mut env_vars_cmd = HashMap::new();
                    for (key, value) in env_vars {
                        env_vars_cmd.insert(key, value);
                    }

                    let env_cmd = Command::Simple(SimpleCommand {
                        name: Word::literal("true".to_string()),
                        args: Vec::new(),
                        redirects: Vec::new(),
                        env_vars: env_vars_cmd,
                        stdout_used: true,
                        stderr_used: true,
                    });

                    Ok(Command::Block(Block {
                        commands: vec![env_cmd, command],
                    }))
                }
            }
        } else {
            // No command following, this is a standalone assignment
            Ok(Command::Assignment(Assignment {
                variable: var_name,
                value: value_word,
                operator: match assignment_op {
                    Token::Assign => AssignmentOperator::Assign,
                    Token::PlusAssign => AssignmentOperator::PlusAssign,
                    Token::MinusAssign => AssignmentOperator::MinusAssign,
                    Token::StarAssign => AssignmentOperator::StarAssign,
                    Token::SlashAssign => AssignmentOperator::SlashAssign,
                    Token::PercentAssign => AssignmentOperator::PercentAssign,
                    _ => AssignmentOperator::Assign, // Default fallback
                },
            }))
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
        // TODO: Implement double paren command parsing
        Err(ParserError::InvalidSyntax(
            "Double paren commands not yet implemented".to_string(),
        ))
    }

    fn parse_shopt_command(&mut self) -> Result<Command, ParserError> {
        // Consume the 'shopt' token
        self.lexer.next();

        // Skip whitespace
        self.lexer.skip_whitespace_and_comments();

        // Parse the option (e.g., -s, -u)
        let enable = if let Some(token) = self.lexer.peek() {
            match token {
                Token::Size => {
                    self.lexer.next();
                    true // -s means set (enable)
                }
                Token::Unset => {
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
            //             eprintln!("DEBUG: Processing token in loop: {:?}", current_token);
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
                    expression_parts.push("-f".to_string());
                    self.lexer.next();
                }
                Some(Token::Directory) => {
                    expression_parts.push("-d".to_string());
                    self.lexer.next();
                }
                Some(Token::Exists) => {
                    expression_parts.push("-e".to_string());
                    self.lexer.next();
                }
                Some(Token::Readable) => {
                    expression_parts.push("-r".to_string());
                    self.lexer.next();
                }
                Some(Token::Writable) => {
                    expression_parts.push("-w".to_string());
                    self.lexer.next();
                }
                Some(Token::Executable) => {
                    expression_parts.push("-x".to_string());
                    self.lexer.next();
                }
                Some(Token::Size) => {
                    expression_parts.push("-s".to_string());
                    self.lexer.next();
                }
                Some(Token::Symlink) => {
                    expression_parts.push("-L".to_string());
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
                Some(Token::CasePattern) => {
                    expression_parts.push(self.lexer.get_raw_token_text()?);
                    self.lexer.next();
                }
                Some(Token::Caret) => {
                    expression_parts.push("^".to_string());
                    self.lexer.next();
                }
                Some(Token::Plus) => {
                    expression_parts.push("+".to_string());
                    self.lexer.next();
                }
                Some(Token::Escape) => {
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
                    self.lexer.next(); // consume the string token
                }
                Some(Token::Space) | Some(Token::Tab) => {
                    self.lexer.next(); // skip whitespace
                }
                Some(Token::Identifier) => {
                    let identifier = self.lexer.get_identifier_text()?;
                    expression_parts.push(identifier);
                    self.lexer.next();
                }
                Some(Token::RegexPattern) => {
                    let pattern_text = self.lexer.get_raw_token_text()?;
                    expression_parts.push(pattern_text);
                    self.lexer.next();
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
                Some(Token::Number) => {
                    let num = self.lexer.get_number_text()?;
                    expression_parts.push(num);
                    self.lexer.next();
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
                    let mut depth = 1usize;
                    loop {
                        match self.lexer.peek() {
                            Some(Token::ArithmeticEvalClose) => {
                                self.lexer.next();
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                                arith.push_str("))");
                            }
                            Some(Token::Arithmetic) | Some(Token::ArithmeticEval) => {
                                self.lexer.next();
                                depth += 1;
                                arith.push_str("$((");
                            }
                            None => break,
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
                Some(Token::Newline) | Some(Token::CarriageReturn) => {
                    // Should not appear inside test expression, treat as end
                    break;
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
                None => {
                    return Err(ParserError::InvalidSyntax(
                        "Unexpected end of input in test expression".to_string(),
                    ));
                }
                _ => {
                    let token_str = format!("{:?}", self.lexer.peek());
                    return Err(ParserError::InvalidSyntax(format!(
                        "Unexpected token in test expression: {}",
                        token_str
                    )));
                }
            }
        }

        let expression = expression_parts.join("");

        Ok(Command::TestExpression(TestExpression {
            expression,
            modifiers: self.get_current_shopt_state(),
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
                            self.lexer.next(); // consume the identifier token
                        }
                        Some(Token::Number) => {
                            // Number like '1'
                            let num_text = self.lexer.get_number_text()?;
                            expression_parts.push(num_text);
                            self.lexer.next(); // consume the number token
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
                        _ => {
                            return Err(ParserError::InvalidSyntax(
                                "Unexpected token in parameter expansion".to_string(),
                            ));
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
pub fn parse_pipeline_from_text(text: &str) -> Result<Command, ParserError> {
    use crate::lexer::{Lexer, Token};

    // Create a lexer for the command text
    let mut lexer = Lexer::new(text);

    // Create a parser with the lexer
    let mut parser = Parser::new_with_lexer(lexer);

    // Parse as a pipeline
    parser.parse_pipeline()
}

// Re-export the main parsing function
pub fn parse(input: &str) -> Result<Vec<Command>, ParserError> {
    let mut parser = Parser::new(input);
    parser.parse()
}
