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
    let bytes = std::fs::read(&args[1]).expect("read input file");
    let content = String::from_utf8_lossy(&bytes);
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
