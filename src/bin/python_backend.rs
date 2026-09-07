//! Python backend CLI — LIBRARY path (branch `backend/python`).
//!
//! Compiles the python renderer IN (debashl::python_backend) and drives it
//! directly from the parsed ShIR — no `--shir` JSON round-trip. Usage:
//!
//!     otranspilerl-cli --target shir foo.sh       # the JSON contract path (core)
//!     python_backend foo.sh                  # this: library path, in-process
//!
//! Debugging aid: prints the generated python to stdout.

use debashl::Parser;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: python_backend <file.sh>");
        std::process::exit(2);
    }
    let bytes = std::fs::read(&args[1]).expect("read input file");
    // lossy: corpus files may be ISO-8859 (the core frontend reads them the
    // same way — the ShIR JSON for such files must still render)
    let content = String::from_utf8_lossy(&bytes).into_owned();
    let commands = match Parser::new(&content).parse() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("parse error: {e}");
            std::process::exit(2);
        }
    };
    let prog = debashl::shir::ast_to_ir(&commands);
    print!("{}", debashl::python_backend::shir_to_python(&prog));
}
