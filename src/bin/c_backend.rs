//! C backend CLI — LIBRARY path (branch `backend/c`).
//!
//! Compiles the C renderer IN (debashl::c_backend) and drives it directly
//! from the parsed ShIR — no `--shir` JSON round-trip. Usage:
//!
//!     debashc file --shir foo.sh            # the JSON contract path (core)
//!     c_backend foo.sh                       # this: library path, in-process
//!     debashc file --estree foo.sh | estree_to_c   # the old ESTree-JSON draft
//!
//! Debugging aid: prints the generated C to stdout.

use debashl::Parser;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: c_backend <file.sh>");
        std::process::exit(2);
    }
    // Corpus files may be ISO-8859-1 etc. (utf8-non-utf8-content.sh);
    // the core parses lossily too, so mirror it here instead of panicking
    // on invalid UTF-8.
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
    print!("{}", debashl::c_backend::shir_to_c(&prog));
}
