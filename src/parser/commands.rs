use crate::ast::*;
use crate::mir::*;
use crate::lexer::{Lexer, Token};
use crate::parser::errors::ParserError;
use crate::parser::utilities::ParserUtilities;
use crate::parser::words::{parse_word, parse_word_no_newline_skip};
use crate::parser::redirects::parse_redirect;
use crate::parser::assignments::parse_array_elements;
use crate::parser::control_flow::{
    parse_if_statement, parse_case_statement, parse_while_loop, parse_for_loop,
    parse_function, parse_posix_function, parse_break_statement, parse_continue_statement,
    parse_return_statement, parse_block
};
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
            let current_token = self.lexer.peek();
            
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
            if matches!(paren1, Some(Token::ParenOpen)) 
                && matches!(paren2, Some(Token::ParenClose)) {
                // Check if the next non-whitespace token is a brace
                let mut pos = 3;
                while pos < 10 && matches!(self.lexer.peek_n(pos), Some(Token::Space | Token::Tab | Token::Comment | Token::Newline)) {
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
                while pos < 10 && matches!(self.lexer.peek_n(pos), Some(Token::Space | Token::Tab | Token::Comment | Token::Newline)) {
                    pos += 1;
                }
                
                // Check for simple assignment: identifier=value
                if matches!(self.lexer.peek_n(pos), Some(Token::Assign | Token::PlusAssign | Token::MinusAssign | Token::StarAssign | Token::SlashAssign | Token::PercentAssign)) {
                    self.parse_standalone_assignment()?
                } else if matches!(self.lexer.peek_n(pos), Some(Token::CasePattern)) {
                    // Check for array subscript assignment: identifier[subscript]=value
                    let mut next_pos = pos + 1;
                    while next_pos < 15 && matches!(self.lexer.peek_n(next_pos), Some(Token::Space | Token::Tab | Token::Comment | Token::Newline)) {
                        next_pos += 1;
                    }
                    if matches!(self.lexer.peek_n(next_pos), Some(Token::Assign | Token::PlusAssign | Token::MinusAssign | Token::StarAssign | Token::SlashAssign | Token::PercentAssign)) {
                        self.parse_standalone_assignment()?
                    } else {
                        self.parse_pipeline()?
                    }
                } else {
                    self.parse_pipeline()?
                }
            }
        } else {
            match self.lexer.peek() {
                Some(Token::Comment) => {
                    // Comments should be handled at the top level
                    return Err(ParserError::InvalidSyntax("Unexpected comment in command parsing".to_string()));
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
                Some(Token::Set) | Some(Token::Unset) | Some(Token::Export) | 
                Some(Token::Readonly) | Some(Token::Declare) | Some(Token::Typeset) |
                Some(Token::Local) | Some(Token::Shift) | Some(Token::Eval) |
                Some(Token::Exec) | Some(Token::Source) | Some(Token::Trap) |
                Some(Token::Wait) | Some(Token::Exit) => self.parse_pipeline()?,
                // Handle redirects at the beginning of a command (e.g., process substitution)
                Some(Token::RedirectIn) | Some(Token::RedirectOut) | Some(Token::RedirectAppend) |
                Some(Token::RedirectInOut) | Some(Token::Heredoc) | Some(Token::HeredocTabs) |
                Some(Token::HereString) | Some(Token::RedirectOutErr) | Some(Token::RedirectInErr) |
                Some(Token::RedirectOutClobber) | Some(Token::RedirectAll) | Some(Token::RedirectAllAppend) => {
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
                Some(Token::ParenOpen) if matches!(self.lexer.peek_n(1), Some(Token::ParenOpen)) => {
                    self.parse_double_paren_command()?
                }
                Some(Token::ParenOpen) => self.parse_subshell()?,
                Some(Token::BraceOpen) => parse_block(self)?,
                Some(Token::TestBracket) => {
                    // Check for double-bracket test [[ ... ]] before parsing as single bracket
                    if matches!(self.lexer.peek_n(1), Some(Token::TestBracket)) {
                        eprintln!("DEBUG: Found double brackets in parse_command, parsing as test expression");
                        // Consume the first two [[ tokens
                        self.lexer.next();
                        self.lexer.next();
                        let test_command = self.parse_test_expression()?;
                        // After parsing the test expression, check if there's a pipeline operator
                        self.lexer.skip_whitespace_and_comments();
                        let next_token = self.lexer.peek();
                        eprintln!("DEBUG: After test expression, next token: {:?}", next_token);
                        if let Some(token) = next_token {
                            match token {
                                Token::And | Token::Or | Token::Pipe => {
                                    eprintln!("DEBUG: Found pipeline operator {:?}, parsing as pipeline", token);
                                    // This is part of a pipeline, parse it as such
                                    // For test expressions, we don't need to capture source text
                                    let dummy_start = 0;
                                    let result = self.parse_pipeline_from_command(test_command, dummy_start)?;
                                    eprintln!("DEBUG: Pipeline parsing result: {:?}", result);
                                    result
                                }
                                _ => {
                                    eprintln!("DEBUG: No pipeline operator, returning test expression");
                                    // Just a test expression, return it
                                    test_command
                                }
                            }
                        } else {
                            eprintln!("DEBUG: No more tokens, returning test expression");
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

        // Check for redirects that follow the command
        self.parse_command_redirects(command)
    }

    fn parse_command_redirects(&mut self, command: Command) -> Result<Command, ParserError> {
        // Check if there are redirects following the command
        let mut redirects = Vec::new();
        
        // Skip whitespace and comments
        self.lexer.skip_whitespace_and_comments();
        
        // Parse redirects until we hit a command separator or other non-redirect token
        while let Some(token) = self.lexer.peek() {
            match token {
                Token::Number | Token::RedirectIn | Token::RedirectOut | Token::RedirectAppend | 
                Token::RedirectInOut | Token::Heredoc | Token::HeredocTabs | Token::HereString |
                Token::RedirectOutErr | Token::RedirectInErr | Token::RedirectOutClobber | 
                Token::RedirectAll | Token::RedirectAllAppend => {
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

    fn parse_pipeline(&mut self) -> Result<Command, ParserError> {
        // Record the starting byte position for source text capture BEFORE parsing the first command
        let start_span = self.lexer.get_span();
        let start_pos = start_span.map(|(start, _)| start).unwrap_or(0);
        
        let first_command = self.parse_simple_command()?;
        // Parse redirects for the first command
        let first_command_with_redirects = self.parse_command_redirects(first_command)?;
        self.parse_pipeline_from_command(first_command_with_redirects, start_pos)
    }

    pub fn parse_pipeline_from_command(&mut self, first_command: Command, start_byte_pos: usize) -> Result<Command, ParserError> {
        let mut commands = Vec::new();
        let mut pipe_operators = Vec::new();
        
        commands.push(first_command);
        
        while let Some(_) = self.lexer.peek() {
            // Skip any whitespace/comments before checking for an operator
            self.lexer.skip_whitespace_and_comments();
            let Some(token) = self.lexer.peek() else { break; };
            match token {
                Token::Pipe => {
                    self.lexer.next();
                    pipe_operators.push(());
                    self.lexer.skip_whitespace_and_comments();
                    let command = self.parse_simple_command()?;
                    // Parse redirects for this command
                    let command_with_redirects = self.parse_command_redirects(command)?;
                    commands.push(command_with_redirects);
                }
                Token::And => {
                    self.lexer.next();
                    self.lexer.skip_whitespace_and_comments();
                    let right_command = self.parse_simple_command()?;
                    let right_command_with_redirects = self.parse_command_redirects(right_command)?;
                    
                    // Create Command::And(left, right)
                    let left_command = commands.pop().unwrap();
                    let and_command = Command::And(Box::new(left_command), Box::new(right_command_with_redirects));
                    commands.push(and_command);
                }
                Token::Or => {
                    self.lexer.next();
                    self.lexer.skip_whitespace_and_comments();
                    let right_command = self.parse_simple_command()?;
                    let right_command_with_redirects = self.parse_command_redirects(right_command)?;
                    
                    // Create Command::Or(left, right)
                    let left_command = commands.pop().unwrap();
                    let or_command = Command::Or(Box::new(left_command), Box::new(right_command_with_redirects));
                    commands.push(or_command);
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
        
        if commands.len() == 1 {
            let result = commands.remove(0);
            eprintln!("DEBUG: parse_pipeline_from_command returning single command: {:?}", result);
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
            
            let result = Command::Pipeline(Pipeline { commands, source_text, stdout_used: true, stderr_used: true });
            eprintln!("DEBUG: parse_pipeline_from_command returning pipeline: {:?}", result);
            Ok(result)
        }
    }

    fn parse_simple_command(&mut self) -> Result<Command, ParserError> {
        // Skip whitespace and comments at the beginning
        self.lexer.skip_whitespace_and_comments();
        


        
        let mut args = Vec::new();
        let redirects = Vec::new();
        let mut env_vars = HashMap::new();
        
        // Parse environment variable-style assignments at the start
        while let Some(token) = self.lexer.peek() {
            match token {
                Token::Identifier => {
                    // Handle compound assignment operators
                    let compound_op = self.lexer.peek_n(1).as_ref().cloned();
                    if let Some(compound_op) = compound_op {
                        match compound_op {
                            Token::PlusAssign => {
                                // Handle array append (var+=(...)) or compound assignment (var+=value)
                                if matches!(self.lexer.peek_n(2), Some(Token::ParenOpen)) {
                                    // Handle array append like: var+=(value)
                                    let var_name = self.lexer.get_identifier_text()?;
                                    self.lexer.next(); // consume +=
                                    self.lexer.next(); // consume (
                                    let elements = parse_array_elements(&mut self.lexer)?;
                                    
                                    let array_word = Word::array(var_name.clone(), elements);
                                    env_vars.insert(var_name, array_word);
                                    self.lexer.skip_whitespace_and_comments();
                                } else {
                                    // Handle compound assignment like: var+=value
                                    let var_name = self.lexer.get_identifier_text()?;
                                    self.lexer.next(); // consume +=
                                    
                                    let value_word = parse_word(&mut self.lexer)?;
                                    
                                    let arithmetic_expr = format!("{}+{}", var_name, value_word.to_string());
                                                        let compound_word = Word::arithmetic(ArithmeticExpression {
                        expression: arithmetic_expr,
                        tokens: vec![],
                    });
                                    
                                    env_vars.insert(var_name, compound_word);
                                    self.lexer.skip_whitespace_and_comments();
                                }
                            }
                            Token::Assign => {
                                if matches!(self.lexer.peek_n(2), Some(Token::ParenOpen)) {
                                    // Handle array declaration like: arr=(one two three)
                                    let var_name = self.lexer.get_identifier_text()?;
                                    self.lexer.next(); // consume =
                                    self.lexer.next(); // consume (
                                    let elements = parse_array_elements(&mut self.lexer)?;
                                    let array_word = Word::array(var_name.clone(), elements);
                                    env_vars.insert(var_name, array_word);
                                    self.lexer.skip_whitespace_and_comments();
                                } else {
                                    // Handle regular assignment like: var=value or map[foo]=bar
                                    let var_name = self.lexer.get_identifier_text()?;
                                    self.lexer.next(); // consume =
                                    let value_word = parse_word(&mut self.lexer)?;
                                    env_vars.insert(var_name, value_word);
                                    self.lexer.skip_whitespace_and_comments();
                                }
                            }
                            _ => {
                                break;
                            }
                        }
                    } else {
                        break;
                    }
                }
                // Handle array subscript assignments like map[foo]=bar
                Token::CasePattern => {
                    // This might be an array subscript assignment like map[foo]=bar
                    // We need to look ahead to see if this is followed by = and a value
                    if let Some(Token::Assign) = self.lexer.peek_n(1) {
                        // This is an array assignment, parse it properly
                        // We need to construct the full array access key
                        // First, get the array name (which should be the previous identifier)
                        // But we need to be careful about the lexer state
                        
                        // Since we're in the middle of parsing assignments, we need to handle this differently
                        // Let's break out and let the main assignment parsing logic handle it
                        break;
                    } else {
                        // Not an assignment, break out of assignment parsing
                        break;
                    }
                }

                _ => break,
            }
        }
        


        // Parse the command name first
        let name = parse_word(&mut self.lexer)?;
        
        // Skip inline whitespace before parsing arguments (but stop at newlines)
        self.lexer.skip_inline_whitespace_and_comments();
        
        // Check if this is a builtin command
        if let Word::Literal(name_str, _) = &name {
            if is_builtin_command(&name_str) {
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
                        Token::RedirectIn | Token::RedirectOut | Token::RedirectAppend | Token::RedirectInErr | Token::RedirectOutErr | Token::RedirectInOut | Token::Heredoc | Token::HeredocTabs | Token::HereString => {
                            break;
                        }
                        Token::Pipe | Token::And | Token::Or | Token::Semicolon | Token::Background => {
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
                Token::RedirectIn | Token::RedirectOut | Token::RedirectAppend | Token::RedirectInErr | Token::RedirectOutErr | Token::RedirectInOut | Token::Heredoc | Token::HeredocTabs | Token::HereString => {
                    break;
                }
                Token::Pipe | Token::And | Token::Or | Token::Semicolon | Token::Background => {
                    break;
                }
                Token::Character | Token::NonZero | Token::Exists | Token::File | Token::Size | Token::Readable | Token::Writable | Token::Executable | Token::NewerThan | Token::OlderThan |
                Token::NameFlag | Token::MaxDepthFlag | Token::TypeFlag | Token::Plus | Token::Minus | Token::Escape => {
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
                                    Token::Identifier | Token::DoubleQuotedString | Token::SingleQuotedString => {
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
                            let current_pos = self.lexer.get_position();
                            
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
        let mut var_name = self.lexer.get_identifier_text()?;
        
        // Check if the next token is a CasePattern (array subscript like [foo])
        if let Some(Token::CasePattern) = self.lexer.peek() {
            // This is an array assignment like map[foo]=bar
            let case_pattern = self.lexer.get_current_text().unwrap_or_default();
            var_name = format!("{}{}", var_name, case_pattern);
            self.lexer.next(); // consume the CasePattern token
        }
        
        // Consume the assignment token (=, +=, -=, etc.)
        let assignment_op = self.lexer.peek().cloned().unwrap();
        match assignment_op {
            Token::Assign | Token::PlusAssign | Token::MinusAssign | Token::StarAssign | Token::SlashAssign | Token::PercentAssign => {
                self.lexer.next();
            }
            _ => return Err(ParserError::InvalidSyntax("Expected assignment operator".to_string())),
        }
        
        // Parse the value
        let value_word = if matches!(self.lexer.peek(), Some(Token::ParenOpen)) {
            // This is an array assignment like arr=(one two three)
            let elements = parse_array_elements(&mut self.lexer)?;
            Word::array(var_name.clone(), elements)
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
            let mut env_vars = HashMap::new();
            env_vars.insert(var_name, value_word);
            
            Ok(Command::Simple(SimpleCommand {
                name: Word::literal("true".to_string()), // Use 'true' as a dummy command
                args: Vec::new(),
                redirects: Vec::new(),
                env_vars,
                stdout_used: true,
                stderr_used: true,
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
                Some(Token::Space | Token::Tab | Token::Comment | Token::Newline | Token::Semicolon | Token::CarriageReturn)
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
            Ok(Command::Subshell(Box::new(Command::Block(Block { commands }))))
        }
    }

    fn parse_double_paren_command(&mut self) -> Result<Command, ParserError> {
        // TODO: Implement double paren command parsing
        Err(ParserError::InvalidSyntax("Double paren commands not yet implemented".to_string()))
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
                    true  // -s means set (enable)
                }
                Token::Unset => {
                    self.lexer.next();
                    false // -u means unset (disable)
                }
                _ => {
                    return Err(ParserError::InvalidSyntax(format!("Expected option after shopt, got: {:?}", token)));
                }
            }
        } else {
            return Err(ParserError::InvalidSyntax("Expected option after shopt".to_string()));
        };
        
        // Skip whitespace
        self.lexer.skip_whitespace_and_comments();
        
        // Parse the option name (e.g., extglob, nocasematch)
        let option_name = if let Some(Token::Identifier) = self.lexer.peek() {
            let name = self.lexer.get_identifier_text()?;
            self.lexer.next();
            name
        } else {
            return Err(ParserError::InvalidSyntax("Expected option name after shopt option".to_string()));
        };
        
        // Update the parser's shell option state
        self.update_shopt_state(&option_name, enable);
        
        Ok(Command::ShoptCommand(ShoptCommand {
            option: option_name,  // Store the option name, not the flag
            enable,               // true for -s, false for -u
        }))
    }

    fn parse_test_expression(&mut self) -> Result<Command, ParserError> {
        use crate::ast::TestExpression;
        
                            // Check if this is being called for double brackets (already consumed) or single bracket
                    // If we're called from double bracket detection, the [[ tokens have already been consumed
                    // If we're called for single bracket, we should see a [ token
                    let is_double_bracket = !matches!(self.lexer.peek(), Some(Token::TestBracket));
                    eprintln!("DEBUG: parse_test_expression called, is_double_bracket: {}, current token: {:?}", is_double_bracket, self.lexer.peek());
                    
                    // If this is a double bracket test, we don't need to consume the opening brackets
                    // If this is a single bracket test, we need to consume the opening [
                    if !is_double_bracket {
                        self.lexer.next(); // consume the [
                    }
        

        
        // Capture the content between brackets
        let mut expression_parts = Vec::new();
        
        eprintln!("DEBUG: Starting to capture expression content, current token: {:?}", self.lexer.peek());
        
        loop {
            let current_token = self.lexer.peek();
            eprintln!("DEBUG: Processing token in loop: {:?}", current_token);
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
                Some(Token::Dollar) => {
                    // Handle variable reference: $variable or regex anchor: $
                    eprintln!("DEBUG: Found $ token, checking if followed by identifier");
                    if let Some(Token::Identifier) = self.lexer.peek_n(1) {
                        // This is a variable reference: $variable
                        eprintln!("DEBUG: Found identifier after $, treating as variable reference");
                        self.lexer.next(); // consume the $
                        let identifier = self.lexer.get_identifier_text()?;
                        eprintln!("DEBUG: Found identifier after $: {}", identifier);
                        expression_parts.push(format!("${}", identifier));
                        self.lexer.next();
                    } else {
                        // This is a regex anchor: $
                        eprintln!("DEBUG: No identifier after $, treating as regex anchor");
                        expression_parts.push("$".to_string());
                        self.lexer.next();
                    }
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
                None => {
                    return Err(ParserError::InvalidSyntax("Unexpected end of input in test expression".to_string()));
                }
                _ => {
                    let token_str = format!("{:?}", self.lexer.peek());
                    return Err(ParserError::InvalidSyntax(format!("Unexpected token in test expression: {}", token_str)));
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
                    Err(ParserError::InvalidSyntax("Expected identifier after $ in variable expansion".to_string()))
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
                            return Err(ParserError::InvalidSyntax("Unexpected end of input in parameter expansion".to_string()));
                        }
                        _ => {
                            return Err(ParserError::InvalidSyntax("Unexpected token in parameter expansion".to_string()));
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
            _ => {
                Err(ParserError::InvalidSyntax("Expected $ or ${ in variable expansion".to_string()))
            }
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
                    return Err(ParserError::InvalidSyntax("Unexpected end of input in arithmetic expression".to_string()));
                }
                _ => {
                    return Err(ParserError::InvalidSyntax("Unexpected token in arithmetic expression".to_string()));
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
    matches!(name, 
        "set" | "unset" | "export" | "readonly" | "declare" | "typeset" | 
        "local" | "shift" | "eval" | "exec" | "source" | "trap" | "wait" | 
        "shopt" | "exit" | "return" | "break" | "continue"
    )
}

// Re-export the main parsing function
pub fn parse(input: &str) -> Result<Vec<Command>, ParserError> {
    let mut parser = Parser::new(input);
    parser.parse()
}

