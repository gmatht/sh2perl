use debashl::{Generator, Lexer, Parser};
use std::fs;
use std::io::Write;
use std::process::Command;

pub fn run_generated(lang: &str, input: &str) {
    let source = if input.ends_with(".sh") || std::path::Path::new(input).exists() {
        fs::read_to_string(input).unwrap_or_else(|_| input.to_string())
    } else {
        input.to_string()
    };

    match lang {
        "perl" => {
            let mut generator = Generator::new();
            let commands = match Parser::new(&source).parse() {
                Ok(c) => c,
                Err(e) => {
                    println!("Parse error: {}", e);
                    return;
                }
            };
            let perl_code = generator.generate(&commands);
            println!("Generated Perl code:");
            println!("{}", "=".repeat(50));
            println!("{}", perl_code);
        }
        _ => println!("Unsupported language for --run: {}", lang),
    }
}

pub fn lex_input(input: &str) {
    let mut lexer = Lexer::new(input);
    let mut token_count = 0;

    println!("Lexing input:");
    println!("{}", "=".repeat(50));

    loop {
        match lexer.next() {
            Some(token) => {
                println!("{:?}", token);
                token_count += 1;
            }
            None => break,
        }
    }

    println!("{}", "=".repeat(50));
    println!("Total tokens: {}", token_count);
}

pub fn parse_input(input: &str) {
    let mut parser = Parser::new(input);

    println!("Parsing input:");
    println!("{}", "=".repeat(50));

    match parser.parse() {
        Ok(commands) => {
            println!("Parse successful!");
            println!("Commands: {:?}", commands);
        }
        Err(e) => {
            println!("Parse error: {}", e);
        }
    }

    println!("{}", "=".repeat(50));
}

pub fn parse_file(filename: &str) {
    match fs::read_to_string(filename) {
        Ok(content) => {
            parse_input(&content);
        }
        Err(e) => {
            println!("Error reading file {}: {}", filename, e);
        }
    }
}

pub fn parse_to_perl(input: &str) {
    let mut generator = Generator::new();

    if debashl::debug::is_debug_enabled() {
        eprintln!("Converting to Perl:");
        eprintln!("{}", "=".repeat(50));
    }

    let commands = match Parser::new(input).parse() {
        Ok(c) => c,
        Err(e) => {
            println!("Parse error: {}", e);
            return;
        }
    };
    let perl_code = generator.generate(&commands);
    println!("Converting to Perl:");
    println!("{}", "=".repeat(50));
    println!("{}", perl_code);
    println!("{}", "=".repeat(50));

    if debashl::debug::is_debug_enabled() {
        eprintln!("{}", "=".repeat(50));
    }
}

pub fn parse_to_perl_inline(input: &str) {
    let mut generator = Generator::new_inline_mode();

    if debashl::debug::is_debug_enabled() {
        println!("Converting to inline Perl:");
        println!("{}", "=".repeat(50));
    }

    let commands = match Parser::new(input).parse() {
        Ok(c) => c,
        Err(e) => {
            println!("Parse error: {}", e);
            return;
        }
    };
    let perl_code = generator.generate(&commands);
    println!("{}", perl_code);

    if debashl::debug::is_debug_enabled() {
        println!("{}", "=".repeat(50));
    }
}

pub fn parse_system_to_perl(input: &str) {
    let mut generator = Generator::new();

    println!("Converting to Perl:");
    println!("{}", "=".repeat(50));

    let commands = match Parser::new(input).parse() {
        Ok(c) => c,
        Err(e) => {
            let wrapped_input = format!("{}", input);
            match Parser::new(&wrapped_input).parse() {
                Ok(c) => c,
                Err(e2) => {
                    println!("Parse error: {}", e);
                    println!("Tried wrapped version, error: {}", e2);
                    return;
                }
            }
        }
    };

    let perl_code = generator.generate(&commands);

    println!("{}", perl_code);

    println!("{}", "=".repeat(50));
}

pub fn parse_backticks_to_perl(input: &str) {
    let mut generator = Generator::new_inline_mode();

    println!("Converting backticks command to Perl:");
    println!("{}", "=".repeat(50));

    let commands = match Parser::new(input).parse() {
        Ok(c) => c,
        Err(e) => {
            let wrapped_input = format!("{}", input);
            match Parser::new(&wrapped_input).parse() {
                Ok(c) => c,
                Err(e2) => {
                    println!("Parse error: {}", e);
                    println!("Tried wrapped version, error: {}", e2);
                    return;
                }
            }
        }
    };

    let perl_code = generator.generate(&commands);

    let clean_code = extract_backticks_perl_logic(&perl_code);
    println!("{}", clean_code);

    println!("{}", "=".repeat(50));
}

fn extract_core_perl_logic(perl_code: &str) -> String {
    if let Some(captures) = regex::Regex::new(r"my \$main_exit_code = 0;\s*\n(.*?)(?:\n\s*$|$)")
        .unwrap()
        .captures(perl_code)
    {
        let code = captures.get(1).unwrap().as_str();
        let cleaned = code.trim_end();
        if cleaned.ends_with(';') {
            cleaned[..cleaned.len() - 1].to_string()
        } else {
            cleaned.to_string()
        }
    } else {
        if let Some(captures) = regex::Regex::new(r"(print.*?;?)\s*$")
            .unwrap()
            .captures(perl_code)
        {
            let code = captures.get(1).unwrap().as_str();
            code.trim_end().to_string()
        } else {
            perl_code.to_string()
        }
    }
}

fn extract_preamble_and_core(perl_code: &str) -> (String, String) {
    if perl_code.contains("@ls_files")
        && perl_code.contains("opendir my $dh")
        && perl_code.contains("$ls_dir = ")
    {
        let preamble = "my @ls_files;\nmy $ls_dir;";

        if let Some(captures) = regex::Regex::new(
            r"(?s)\$ls_dir = '([^']+)';\s*\n@ls_files = \(\);\s*\n(.*?)(print.*?;?)\s*$",
        )
        .unwrap()
        .captures(perl_code)
        {
            let dir = captures.get(1).unwrap().as_str();
            let opendir_logic = captures.get(2).unwrap().as_str().trim();
            let print_stmt = captures.get(3).unwrap().as_str();
            let core_code = format!(
                "$ls_dir = '{}';\n@ls_files = ();\n{}\n{}",
                dir, opendir_logic, print_stmt
            );
            let final_core = if core_code.ends_with(';') {
                core_code[..core_code.len() - 1].to_string()
            } else {
                core_code.to_string()
            };
            return (preamble.to_string(), final_core);
        }

        if let Some(captures) =
            regex::Regex::new(r"my \$ls_dir = '([^']+)';\n@ls_files = \(\);\n(.*?)(print.*?;?)\s*$")
                .unwrap()
                .captures(perl_code)
        {
            let dir = captures.get(1).unwrap().as_str();
            let opendir_logic = captures.get(2).unwrap().as_str().trim();
            let print_stmt = captures.get(3).unwrap().as_str();
            let core_code = format!(
                "$ls_dir = '{}';\n@ls_files = ();\n{}\n{}",
                dir, opendir_logic, print_stmt
            );
            let final_core = if core_code.ends_with(';') {
                core_code[..core_code.len() - 1].to_string()
            } else {
                core_code.to_string()
            };
            return (preamble.to_string(), final_core);
        }
    }

    if let Some(captures) =
        regex::Regex::new(r"(.*?my \$main_exit_code = 0;\s*\n)(.*?)(?:\n\s*$|$)")
            .unwrap()
            .captures(perl_code)
    {
        let preamble = captures.get(1).unwrap().as_str().trim().to_string();
        let core_code = captures.get(2).unwrap().as_str();
        let cleaned = core_code.trim_end();
        let final_core = if cleaned.ends_with(';') {
            cleaned[..cleaned.len() - 1].to_string()
        } else {
            cleaned.to_string()
        };
        return (preamble, final_core);
    }

    if let Some(captures) = regex::Regex::new(r"(?s)(.*?)(my @[^;]+;.*?)(print.*?;?)\s*$")
        .unwrap()
        .captures(perl_code)
    {
        let header = captures.get(1).unwrap().as_str().trim().to_string();
        let var_decls = captures.get(2).unwrap().as_str().trim().to_string();
        let core_code = captures.get(3).unwrap().as_str().trim().to_string();

        let preamble = if header.is_empty() {
            var_decls
        } else {
            format!("{}\n{}", header, var_decls)
        };

        let final_core = if core_code.ends_with(';') {
            core_code[..core_code.len() - 1].to_string()
        } else {
            core_code.to_string()
        };

        return (preamble, final_core);
    }

    if let Some(captures) = regex::Regex::new(r"(print.*?;?)\s*$")
        .unwrap()
        .captures(perl_code)
    {
        let code = captures.get(1).unwrap().as_str();
        return ("".to_string(), code.trim_end().to_string());
    }

    ("".to_string(), perl_code.to_string())
}

fn extract_backticks_perl_logic(perl_code: &str) -> String {
    if let Some(captures) = regex::Regex::new(r"my \$main_exit_code = 0;\s*\n(.*?)(?:\n\s*$|$)")
        .unwrap()
        .captures(perl_code)
    {
        let code = captures.get(1).unwrap().as_str();
        let modified_code = code.replace("print ", "`");
        let cleaned = modified_code.trim_end();
        if cleaned.ends_with(';') {
            let result = cleaned[..cleaned.len() - 1].to_string();
            if result.ends_with('`') {
                result
            } else {
                let without_semicolon = result.replace(";`", "`");
                without_semicolon
            }
        } else {
            if cleaned.ends_with('`') {
                cleaned.to_string()
            } else {
                format!("{}`", cleaned)
            }
        }
    } else {
        if let Some(captures) = regex::Regex::new(r"(print.*?;?)\s*$")
            .unwrap()
            .captures(perl_code)
        {
            let code = captures.get(1).unwrap().as_str();
            let modified_code = code.replace("print ", "`");
            let result = modified_code.trim_end().to_string();
            if result.ends_with('`') {
                result
            } else {
                let with_backtick = format!("{}`", result);
                with_backtick.replace(";`", "`")
            }
        } else {
            perl_code.to_string()
        }
    }
}

pub fn parse_file_to_perl(filename: &str) {
    match fs::read_to_string(filename) {
        Ok(content) => {
            parse_to_perl(&content);
        }
        Err(e) => {
            println!("Error reading file {}: {}", filename, e);
        }
    }
}

pub fn interactive_mode() {
    println!("Interactive mode - type 'quit' to exit");
    println!("{}", "=".repeat(50));

    loop {
        print!("> ");
        std::io::stdout().flush().unwrap();

        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        let input = input.trim();

        if input == "quit" {
            break;
        }

        if input.is_empty() {
            continue;
        }

        match input {
            "help" => {
                println!("Available commands:");
                println!("  help - show this help");
                println!("  quit - exit interactive mode");
                println!("  <shell code> - parse and convert to Perl");
            }
            _ => {
                parse_to_perl(input);
            }
        }
    }
}

pub fn export_mir(input: &str, optimize: bool) {
    println!("MIR Export:");
    println!("{}", "=".repeat(50));

    let commands = match Parser::new(input).parse() {
        Ok(c) => c,
        Err(e) => {
            println!("Parse error: {}", e);
            return;
        }
    };

    if optimize {
        println!("Optimized MIR:");
        for (i, cmd) in commands.iter().enumerate() {
            println!("Command {}: {:#?}", i, cmd);
        }
    } else {
        println!("MIR Commands:");
        for (i, cmd) in commands.iter().enumerate() {
            println!("Command {}: {:#?}", i, cmd);
        }
    }

    println!("{}", "=".repeat(50));
}

pub fn export_mir_to_json(input: &str, _optimize: bool) {
    let commands = match Parser::new(input).parse() {
        Ok(c) => c,
        Err(e) => {
            println!("Parse error: {}", e);
            return;
        }
    };

    match serde_json::to_string_pretty(&commands) {
        Ok(json) => println!("{}", json),
        Err(e) => println!("JSON serialization error: {}", e),
    }
}

pub fn parse_perl_critic_only(input: &str) {
    let lex_result = test_perl_lex(input);
    if lex_result != 0 {
        std::process::exit(101);
    }

    let parse_result = test_perl_parse(input);
    if parse_result != 0 {
        std::process::exit(102);
    }

    let generate_result = test_perl_generate(input);
    if generate_result != 0 {
        std::process::exit(104);
    }

    let critic_result = test_perl_critic(input);
    if critic_result != 0 {
        std::process::exit(137);
    }

    std::process::exit(0);
}

fn test_perl_lex(input: &str) -> i32 {
    let child = Command::new("perl")
        .arg("-c")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .spawn();

    match child {
        Ok(mut child) => {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(input.as_bytes());
            }
            match child.wait() {
                Ok(status) => status.code().unwrap_or(1),
                Err(_) => 1,
            }
        }
        Err(_) => 1,
    }
}

fn test_perl_parse(input: &str) -> i32 {
    test_perl_lex(input)
}

fn test_perl_generate(input: &str) -> i32 {
    let child = Command::new("perl")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .spawn();

    match child {
        Ok(mut child) => {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(input.as_bytes());
            }
            match child.wait() {
                Ok(status) => status.code().unwrap_or(1),
                Err(_) => 1,
            }
        }
        Err(_) => 1,
    }
}

fn test_perl_critic(input: &str) -> i32 {
    let temp_file = "__tmp_perl_critic_test.pl";
    if let Err(_) = fs::write(temp_file, input) {
        return 1;
    }

    let output = Command::new("perl")
        .arg("perlcritic_wrapper.pl")
        .arg(temp_file)
        .output();

    let _ = fs::remove_file(temp_file);

    match output {
        Ok(child) => child.status.code().unwrap_or(1),
        Err(_) => 1,
    }
}
