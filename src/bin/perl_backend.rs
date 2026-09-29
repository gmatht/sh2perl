//! Perl backend CLI — LIBRARY path.
//!
//! DEDUP (2026-09-29): this used to call `debashl::perl_backend`, a partial
//! renderer rewrite that `aa583efab` swept in inside an unrelated estree/C
//! commit and that was never finished. It punted 60 `# TODO(unsupported)`
//! markers across 30 corpus examples where the renderer in `ir.rs` punts 0,
//! and shell-outs instead of lowering (measured 7/20 vs 19/20 against bash).
//! `perl_backend.rs` is deleted; `ir::shir_to_perl` is the only Perl renderer.
//!
//! Compiles the Perl renderer IN (debashl::ir) and drives it
//! directly from the parsed ShIR — no `--shir` JSON round-trip. Usage:
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
    if args.len() < 2 {
        eprintln!("usage: perl_backend <file.sh>");
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
    print!("{}", debashl::ir::shir_to_perl(&prog));
}
