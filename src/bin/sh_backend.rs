//! sh backend CLI — LIBRARY path (branch `backend/sh`).
//!
//! Compiles the pure-POSIX-sh renderer IN (debashl::sh_backend) and
//! drives it directly from the parsed ShIR — no JSON round-trip. Usage:
//!
//!     sh_backend foo.sh        # in-process: parse -> ast_to_ir -> shir_to_sh
//!
//! The corpus gate (`setup_backends.sh --backend-gate sh`) runs this per
//! example and requires exit 0 + valid POSIX-sh on stdout.

use debashl::Parser;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: sh_backend <file.sh>");
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
    match debashl::sh_backend::shir_to_sh(&prog) {
        Ok(sh) => print!("{sh}"),
        Err(e) => {
            eprintln!("unsupported (v1 subset): {e}");
            std::process::exit(1);
        }
    }
}
