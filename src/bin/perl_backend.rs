//! Perl backend CLI — LIBRARY path.
//!
//! Drives the ONE Perl renderer (`debashl::ir::shir_to_perl`, the same
//! implementation the library, `shir_render --target perl`, the wasi API
//! and `perl_generator_fixed` use) directly from the parsed ShIR — no
//! `--shir` JSON round-trip. Usage:
//!
//!     perl_backend foo.sh                # this: library path, in-process
//!     otranspilerl-cli --source-lang shir --target perl - < foo.json  # the JSON contract path (core)
//!
//! The renderer NEVER fails on valid shell input (unsupported constructs
//! become `# TODO(unsupported)` markers), so the corpus gate's render
//! step always succeeds.

use debashl::Parser;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    // `--fixed` renders through the shipped `--target pl` path
    // (`perl_generator_fixed::generate` = this renderer + the IR/text
    // fixups); the bare form renders `ir::shir_to_perl` alone, which is
    // what pins the renderer in unit tests.
    let fixed = args.get(1).map(|a| a == "--fixed").unwrap_or(false);
    let file = if fixed { args.get(2) } else { args.get(1) };
    let Some(file) = file else {
        eprintln!("usage: perl_backend [--fixed] <file.sh>");
        std::process::exit(2);
    };
    // Corpus files may be ISO-8859-1 etc. (utf8-non-utf8-content.sh);
    // the core parses lossily too, so mirror it here instead of panicking
    // on invalid UTF-8.
    let bytes = std::fs::read(file).expect("read input file");
    let content = String::from_utf8_lossy(&bytes);
    let commands = match Parser::new(&content).parse() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("parse error: {e}");
            std::process::exit(2);
        }
    };
    let prog = debashl::shir::ast_to_ir(&commands);
    let out = if fixed {
        debashl::perl_generator_fixed::generate(&prog)
    } else {
        debashl::ir::shir_to_perl(&prog)
    };
    print!("{out}");
}
