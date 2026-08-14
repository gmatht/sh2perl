use debashl::ast::Word;
use debashl::mir_simple::MirCommand;
use debashl::{Generator, Lexer, Parser};

/// The ESTree fallback emitted when the parser REJECTS a script: bash also
/// rejects it (syntax error — every corpus file that reaches this path has
/// bash exit 2 and empty stdout), so the transpiled program must reproduce
/// the verdict: no stdout, exit 2 (the stderr diagnostic is not compared).
/// Before this fallback the CLI printed nothing on stdout, the gate
/// materialized an empty Program, and the runner exited 0 — "exit code
/// (bash=2 estree=0)" failures for every parse-error corpus test.
fn parse_error_estree_fallback() -> String {
    serde_json::json!({
        "type": "Program",
        "sourceType": "module",
        "body": [{
            "type": "ExpressionStatement",
            "expression": {
                "type": "CallExpression",
                "callee": {
                    "type": "MemberExpression",
                    "object": {"type": "Identifier", "name": "process"},
                    "property": {"type": "Identifier", "name": "exit"},
                    "computed": false,
                    "optional": false
                },
                "arguments": [{"type": "Literal", "value": 2, "raw": "2"}],
                "optional": false
            }
        }]
    })
    .to_string()
}
use std::fs;
use std::io::Read;
use std::io::Write;
use std::process::Command;

/// Read a `file`-style input: `-` means stdin (virtual stdin first — the
/// wasm embedders set it — then real fd 0); anything else is fs::read.
/// Byte-preserving so the ESTree path keeps its raw-byte handling.
pub(crate) fn read_cli_input(filename: &str) -> std::io::Result<Vec<u8>> {
    if filename != "-" {
        return fs::read(filename);
    }
    crate::with_virtual_stdin(|opt| match opt {
        Some(bytes) => Ok(bytes.to_vec()),
        None => {
            let mut buf = Vec::new();
            std::io::stdin().read_to_end(&mut buf)?;
            Ok(buf)
        }
    })
}

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
            // TODO: Fix error handling for position information
        }
    }

    println!("{}", "=".repeat(50));
}

pub fn parse_file(filename: &str) {
    match read_cli_input(filename) {
        Ok(bytes) => {
            // Preserve invalid bytes as PUA markers so the generator can
            // re-emit them as `\xNN` byte escapes (byte-exact vs bash).
            parse_input(&debashl::shared_utils::SharedUtils::bytes_to_marked_lossy(&bytes));
        }
        Err(e) => {
            println!("Error reading file {}: {}", filename, e);
        }
    }
}

pub fn parse_to_perl(input: &str) {
    // Magic numbers are off by default (constructor sets no_magic_numbers=true).
    parse_to_perl_with_opts(input, None);
}

pub fn parse_to_perl_with_opts(input: &str, no_magic_numbers: Option<bool>) {
    // The Perl backend consumes the shIR (the universal contract), NOT the
    // AST directly — the AST-side Generator is the legacy text-builder that
    // the shIR renderer supersedes (PLAN §3).  parse → ast_to_ir →
    // shir_to_perl.  `no_magic_numbers` is a legacy-generator option the
    // shIR path ignores (the shIR renderer never emits magic-number
    // constants).
    let _ = no_magic_numbers;

    // Check if debug is enabled before printing debug output
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
    let prog = debashl::shir::ast_to_ir(&commands);
    let perl_code = debashl::ir::shir_to_perl(&prog);
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
    let perl_code = if commands.len() == 1 {
        generator.word_to_perl(&Word::CommandSubstitution(
            Box::new(commands[0].clone()),
            None,
        ))
    } else {
        generator.generate(&commands)
    };
    println!("{}", perl_code);

    if debashl::debug::is_debug_enabled() {
        println!("{}", "=".repeat(50));
    }
}

pub fn parse_system_to_perl(input: &str) {
    let mut generator = Generator::new();

    println!("Converting to Perl:");
    println!("{}", "=".repeat(50));

    // For system commands, we need to be more lenient with parsing
    // Try to parse as-is first
    let commands = match Parser::new(input).parse() {
        Ok(c) => c,
        Err(e) => {
            // If parsing fails, try to wrap in a simple command structure
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
    let mut generator = Generator::new();

    println!("Converting backticks command to Perl:");
    println!("{}", "=".repeat(50));

    // For backticks, we need to generate code that captures output
    let commands = match Parser::new(input).parse() {
        Ok(c) => c,
        Err(e) => {
            // If parsing fails, try to wrap in a simple command structure
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

    // For backticks, we need to modify the output to capture it
    let clean_code = extract_backticks_perl_logic(&perl_code);
    println!("{}", clean_code);

    println!("{}", "=".repeat(50));
}

fn extract_core_perl_logic(perl_code: &str) -> String {
    // Look for the main logic after variable declarations
    if let Some(captures) = regex::Regex::new(r"my \$main_exit_code = 0;\s*\n(.*?)(?:\n\s*$|$)")
        .unwrap()
        .captures(perl_code)
    {
        let code = captures.get(1).unwrap().as_str();
        // Clean up the code - remove trailing semicolons and extra whitespace
        let cleaned = code.trim_end();
        if cleaned.ends_with(';') {
            cleaned[..cleaned.len() - 1].to_string()
        } else {
            cleaned.to_string()
        }
    } else {
        // If we can't find the pattern, try to extract just the core logic
        // Look for print statements or other core logic
        if let Some(captures) = regex::Regex::new(r"(print.*?;?)\s*$")
            .unwrap()
            .captures(perl_code)
        {
            let code = captures.get(1).unwrap().as_str();
            code.trim_end().to_string()
        } else {
            // Return the original code if we can't extract anything
            perl_code.to_string()
        }
    }
}

fn extract_preamble_and_core(perl_code: &str) -> (String, String) {
    // Check if this is an ls command by looking for ls-specific patterns FIRST
    // (before checking for full Perl script, so ls commands get special handling)
    if perl_code.contains("@ls_files")
        && perl_code.contains("opendir my $dh")
        && perl_code.contains("$ls_dir = ")
    {
        // This is an ls command - generate generic preamble (just variable declarations) and extract core logic
        let preamble = "my @ls_files;\nmy $ls_dir;";

        // Extract the core logic (directory assignment, opendir logic, and print statement)
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

        // Alternative pattern: look for the directory assignment in the preamble and print in core
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

    // Look for the main logic after variable declarations
    if let Some(captures) =
        regex::Regex::new(r"(.*?my \$main_exit_code = 0;\s*\n)(.*?)(?:\n\s*$|$)")
            .unwrap()
            .captures(perl_code)
    {
        let preamble = captures.get(1).unwrap().as_str().trim().to_string();
        let core_code = captures.get(2).unwrap().as_str();
        // Clean up the core code - remove trailing semicolons and extra whitespace
        let cleaned = core_code.trim_end();
        let final_core = if cleaned.ends_with(';') {
            cleaned[..cleaned.len() - 1].to_string()
        } else {
            cleaned.to_string()
        };
        return (preamble, final_core);
    }

    // Try to extract variable declarations and core logic separately
    // Look for variable declarations (my @...; or my $...;) followed by the main logic
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

    // If we can't find the pattern, try to extract just the core logic
    // Look for print statements or other core logic
    if let Some(captures) = regex::Regex::new(r"(print.*?;?)\s*$")
        .unwrap()
        .captures(perl_code)
    {
        let code = captures.get(1).unwrap().as_str();
        return ("".to_string(), code.trim_end().to_string());
    }

    // Default fallback - return original code as core with empty preamble
    ("".to_string(), perl_code.to_string())
}

fn extract_backticks_perl_logic(perl_code: &str) -> String {
    // For backticks, we need to capture the output instead of just printing it
    // Look for the main logic after variable declarations
    if let Some(captures) = regex::Regex::new(r"my \$main_exit_code = 0;\s*\n(.*?)(?:\n\s*$|$)")
        .unwrap()
        .captures(perl_code)
    {
        let code = captures.get(1).unwrap().as_str();
        // Convert print statements to capture output using backticks
        let modified_code = code.replace("print ", "`");
        let cleaned = modified_code.trim_end();
        if cleaned.ends_with(';') {
            let result = cleaned[..cleaned.len() - 1].to_string();
            if result.ends_with('`') {
                result
            } else {
                // Remove any trailing semicolon from the command part
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
        // If we can't find the pattern, try to extract and modify print statements
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
                // Remove any trailing semicolon from the command part
                with_backtick.replace(";`", "`")
            }
        } else {
            // Return the original code if we can't extract anything
            perl_code.to_string()
        }
    }
}

pub fn parse_file_to_perl(filename: &str) {
    match read_cli_input(filename) {
        Ok(bytes) => {
            parse_to_perl(&debashl::shared_utils::SharedUtils::bytes_to_marked_lossy(&bytes));
        }
        Err(e) => {
            println!("Error reading file {}: {}", filename, e);
        }
    }
}

/// Parse a shell file and emit raw ESTree JSON (no perl exec, no diff).
/// For machine consumers / byte-equality tests. Plan improvement #6.
pub fn parse_file_to_estree_raw(filename: &str) {
    match read_cli_input(filename) {
        Ok(bytes) => {
            let content = match String::from_utf8(bytes.clone()) {
                Ok(s) => s,
                Err(_) => bytes.iter().map(|&b| {
                    if b < 0x80 { b as char } else { char::from_u32(0xF800 + b as u32).unwrap_or('?') }
                }).collect(),
            };
            let commands = match debashl::Parser::new(&content).parse() {
                Ok(c) => c,
                Err(e) => { eprintln!("Parse error: {}", e); return; }
            };
            let prog = debashl::shir::ast_to_ir(&commands);
            match debashl::shir::shir_to_estree_json(&prog) {
                Ok(s) => print!("{}", s),
                Err(e) => { eprintln!("estree: {}", e); std::process::exit(1); }
            }
        }
        Err(e) => { eprintln!("read {}: {}", filename, e); std::process::exit(1); }
    }
}

/// Parse a shell file and emit **standard ESTree JSON** (v0 backend,
/// `sh2.*` runtime namespace — see src/estree.rs / PLAN.md §1.2).
pub fn parse_file_to_estree(filename: &str) {
    match read_cli_input(filename) {
        Ok(bytes) => {
            // bash reads the source as raw bytes and passes non-UTF-8 bytes
            // through unchanged (utf8-non-utf8-content.sh: a lone Latin-1
            // byte). fs::read_to_string would reject such files, so decode
            // byte-preservingly: valid UTF-8 as-is; otherwise map bytes
            // >= 0x80 to U+F800+byte private-use chars, which the emitter
            // (src/estree.rs map_raw_bytes) turns into raw-byte markers the
            // runtime writes back byte-for-byte.
            let content = match String::from_utf8(bytes.clone()) {
                Ok(s) => s,
                Err(_) => bytes
                    .iter()
                    .map(|&b| {
                        if b < 0x80 {
                            b as char
                        } else {
                            char::from_u32(0xF800 + b as u32).unwrap_or('\u{FFFD}')
                        }
                    })
                    .collect(),
            };
            let commands = match Parser::new(&content).parse() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Parse error: {}", e);
                    // bash rejects the same file (syntax error, exit 2, no
                    // stdout) — emit the exit-2 fallback program so the
                    // runner's verdict matches bash's.
                    println!("{}", parse_error_estree_fallback());
                    return;
                }
            };
            match debashl::estree::ast_to_estree_json(&commands) {
                Ok(json) => {
                    // Opt-in bc → native Math.* lowering (cli/src/bc_native.rs):
                    // `x=$(echo 'expr' | bc -l)` becomes String(Math.*) with
                    // zero spawns. Env-gated so the corpus output stays
                    // byte-identical by default (SH2_BC_NATIVE=1 to enable).
                    let json = if std::env::var("SH2_BC_NATIVE").is_ok() {
                        crate::bc_native::lower_bc_native(&json)
                    } else {
                        json
                    };
                    // Opt-in source-name $0 semantic (`--argv0-source <name>`):
                    // bake `sh2.argv0 = '<name>'` as the first statement so the
                    // translated JS identifies as the ORIGINAL bash file, whatever
                    // the executor's temp file is called. Default (flag absent) =
                    // argv0 pass-through — the executor supplies argv0 at run time
                    // (estree-runner --source/--name). See
                    // harness/argv0-tests/README.md for the two semantics.
                    let json = if let Some(name) = crate::argv0_source() {
                        match serde_json::from_str::<serde_json::Value>(&json) {
                            Ok(mut v) => {
                                let assign = serde_json::json!({
                                    "type": "ExpressionStatement",
                                    "expression": {
                                        "type": "AssignmentExpression",
                                        "operator": "=",
                                        "left": {
                                            "type": "MemberExpression",
                                            "object": {"type": "Identifier", "name": "sh2"},
                                            "property": {"type": "Identifier", "name": "argv0"},
                                            "computed": false,
                                            "optional": false
                                        },
                                        "right": {"type": "Literal", "value": name}
                                    }
                                });
                                if let Some(body) = v.get_mut("body").and_then(|b| b.as_array_mut()) {
                                    body.insert(0, assign);
                                }
                                v.to_string()
                            }
                            Err(_) => json,
                        }
                    } else {
                        json
                    };
                    println!("{}", json);
                }
                Err(e) => eprintln!("ESTree serialization error: {}", e),
            }
        }
        Err(e) => {
            eprintln!("Error reading file {}: {}", filename, e);
        }
    }
}

/// Parse a shell file and emit **ShIR JSON** (ask A1) — the
/// language-neutral serialized IR for non-Rust backends (C, python, zig,
/// go). Includes var-type verdicts (A2) and purity classification (A3).
/// See src/shir_json.rs / docs/backend-c-core-needs.md §8.
pub fn parse_file_to_shir(filename: &str) {
    let commands = match read_cli_input(filename) {
        Ok(bytes) => {
            // marked-lossy decode (core request
            // perl-20260814-175710): an invalid-UTF-8 byte must not
            // collapse the whole file to an empty program — bash is
            // byte-agnostic, so parse the text with each invalid byte
            // preserved as a PUA marker (U+E000+byte) that serde_json
            // round-trips into the A1 JSON, exactly like
            // parse_file_to_perl above. Backends decode the marker
            // back to `\xNN` byte escapes for byte-exact output.
            let content =
                debashl::shared_utils::SharedUtils::bytes_to_marked_lossy(&bytes);
            match Parser::new(&content).parse() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Parse error: {}", e);
                    // parse-gaps core request: a parse failure must never
                    // produce EMPTY stdout (frontends die "invalid JSON:
                    // EOF"). Emit the canonical empty Program — the same
                    // shape `--shir` produces for an empty file — so the
                    // A1 contract stays ingestible. Genuinely-incomplete
                    // scripts (parse-paren-after-do.sh, parse-unexpected-
                    // end-of-input.sh) are faithfully an empty program
                    // (bash rejects them too).
                    println!(
                        "{}",
                        debashl::shir_json::shir_to_shir_json(&debashl::shir::ast_to_ir(&[]))
                    );
                    return;
                }
            }
        }
        Err(e) => {
            eprintln!("Error reading file {}: {}", filename, e);
            return;
        }
    };
    let prog = debashl::shir::ast_to_ir(&commands);
    println!("{}", debashl::shir_json::shir_to_shir_json(&prog));
}

/// Plan improvement #5: ingest ShIR JSON and emit Perl source
/// via the Perl backend (which consumes the same IrProgram as the
/// neutral shIR — no bridge needed). Closes the frontend → shIR → Perl
/// path for arbitrary source languages.
pub fn parse_shir_json_to_perl(filename: &str) {
    let content = match std::fs::read_to_string(filename) {
        Ok(c) => c,
        Err(e) => { eprintln!("read {}: {}", filename, e); return; }
    };
    let prog = match debashl::shir_json_in::shir_json_to_ir(&content) {
        Ok(p) => p,
        Err(e) => { eprintln!("ShIR JSON ingress: {}", e); std::process::exit(1); }
    };
    let perl = debashl::ir::shir_to_perl(&prog);
    print!("{}", perl);
}

/// Parse shell input and emit ShIR JSON. `raw=true` omits the trailing
/// newline (the contract for machine consumers); `raw=false` adds it
/// (human-readable default). Fixes the long-standing --shir --raw lie.
pub fn export_shir(input: &str, raw: bool) {
    let commands = match Parser::new(input).parse() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Parse error: {}", e);
            // parse-gaps core request: graceful fallback — a parse failure
            // emits the canonical empty Program (never empty stdout, which
            // frontends read as "invalid JSON: EOF").
            let json = debashl::shir_json::shir_to_shir_json(&debashl::shir::ast_to_ir(&[]));
            if raw { print!("{}", json); } else { println!("{}", json); }
            return;
        }
    };
    let prog = debashl::shir::ast_to_ir(&commands);
    let json = debashl::shir_json::shir_to_shir_json(&prog);
    if raw { print!("{}", json); } else { println!("{}", json); }
}

/// Plan §2.3: raw export (unoptimized, no A2 var_types). Always raw
/// (no trailing newline) — by definition. Pins F(S)_raw == C(S)_raw.
pub fn export_shir_raw(input: &str) {
    let commands = match Parser::new(input).parse() {
        Ok(c) => c,
        Err(e) => { eprintln!("Parse error: {}", e); return; }
    };
    let prog = debashl::shir::ast_to_ir_raw(&commands);
    print!("{}", debashl::shir_json::shir_to_shir_json_raw(&prog));
}

/// Plan §2.2: ingest a ShIR JSON file, run it through the ESTree
/// backend. Closes the pipe (frontend JSON → core → backend). The
/// Perl backend consumes its own IR flavor and cannot ingest the
/// neutral ShIR directly (see plan §1.3).
pub fn parse_shir_json_to_estree(filename: &str) {
    let content = match std::fs::read_to_string(filename) {
        Ok(c) => c,
        Err(e) => { eprintln!("read {}: {}", filename, e); return; }
    };
    let prog = match debashl::shir_json_in::shir_json_to_ir(&content) {
        Ok(p) => p,
        Err(e) => { eprintln!("ShIR JSON ingress: {}", e); std::process::exit(1); }
    };
    match debashl::shir::shir_to_estree_json(&prog) {
        Ok(s) => println!("{}", s),
        Err(e) => { eprintln!("estree: {}", e); std::process::exit(1); }
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

    // Convert AST commands to MIR commands
    let mir_commands: Vec<MirCommand> = commands
        .iter()
        .map(|cmd| MirCommand::from_ast_command(cmd))
        .collect();

    if optimize {
        println!("Optimized MIR:");
        // TODO: Add optimization passes here
        for (i, mir_cmd) in mir_commands.iter().enumerate() {
            println!("Command {}: {:?}", i, mir_cmd);
        }
    } else {
        println!("MIR Commands:");
        for (i, mir_cmd) in mir_commands.iter().enumerate() {
            println!("Command {}: {:?}", i, mir_cmd);
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

    // Convert AST commands to MIR commands
    let mir_commands: Vec<MirCommand> = commands
        .iter()
        .map(|cmd| MirCommand::from_ast_command(cmd))
        .collect();

    match serde_json::to_string_pretty(&mir_commands) {
        Ok(json) => println!("{}", json),
        Err(e) => println!("JSON serialization error: {}", e),
    }
}

pub fn parse_perl_critic_only(input: &str) {
    // Test if the input can be lexed (syntax check)
    let lex_result = test_perl_lex(input);
    if lex_result != 0 {
        std::process::exit(101); // Lex failure
    }

    // Test if the input can be parsed (compilation check)
    let parse_result = test_perl_parse(input);
    if parse_result != 0 {
        std::process::exit(102); // Parse failure
    }

    // Test if the input can be generated/executed
    let generate_result = test_perl_generate(input);
    if generate_result != 0 {
        std::process::exit(104); // Generate failure
    }

    // Test if the generated code passes Perl Critic
    let critic_result = test_perl_critic(input);
    if critic_result != 0 {
        std::process::exit(137); // Perl Critic failure
    }

    // All tests passed
    std::process::exit(0);
}

fn test_perl_lex(input: &str) -> i32 {
    // Test basic syntax with perl -c
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
    // Test compilation with perl -c (same as syntax for now)
    test_perl_lex(input)
}

fn test_perl_generate(input: &str) -> i32 {
    // Test if the code can be executed without errors
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
    // Write input to temporary file
    let temp_file = "__tmp_perl_critic_test.pl";
    if let Err(_) = fs::write(temp_file, input) {
        return 1;
    }

    // Run Perl Critic on the file
    let output = Command::new("perl")
        .arg("perlcritic_wrapper.pl")
        .arg(temp_file)
        .output();

    // Clean up temporary file
    let _ = fs::remove_file(temp_file);

    match output {
        Ok(child) => child.status.code().unwrap_or(1),
        Err(_) => 1,
    }
}
