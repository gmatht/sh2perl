//! rust backend CLI — LIBRARY path.
//!
//! Compiles the Rust renderer IN (debashl::rust_backend) and drives it
//! directly from the parsed ShIR — no JSON round-trip. Usage:
//!
//!     rust_backend foo.sh        # in-process: parse -> ast_to_ir -> shir_to_rust
//!
//! The corpus gate runs this per example and requires exit 0 + valid Rust
//! on stdout.

use debashl::Parser;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: rust_backend <file.sh>");
        std::process::exit(2);
    }
    // Corpus files may be ISO-8859-1 etc. (utf8-non-utf8-content.sh).
    // `from_utf8_lossy` replaced the raw 0xE9 byte with U+FFFD, so the
    // emitted Rust carried `EF BF BD` instead of `E9` and stdout diverged
    // from bash. Decode to the same PUA marker the core's read_source uses
    // (0xF800+byte) and that rust_backend's `raw_byte_of` already lowers
    // back to the raw byte; valid UTF-8 passes through untouched.
    let bytes = std::fs::read(&args[1]).expect("read input file");
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
            eprintln!("parse error: {e}");
            std::process::exit(2);
        }
    };
    let prog = debashl::shir::ast_to_ir(&commands);
    print!("{}", debashl::rust_backend::shir_to_rust(&prog));
}
