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
    // Corpus files may be ISO-8859-1 etc. (utf8-non-utf8-content.sh). bash
    // and Perl treat the script as BYTES, so a `0xE9` in a string literal has
    // to come back out as `0xE9`. A lossy decode replaced it with U+FFFD and
    // printed `EF BF BD` where bash printed the original byte. Decode into a
    // String that round-trips invalid bytes (private-use area) and re-encode
    // on the way out.
    let bytes = std::fs::read(file).expect("read input file");
    let content = decode_bytes(&bytes);
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
    use std::io::Write;
    let mut stdout = std::io::stdout();
    stdout
        .write_all(&encode_bytes(&out))
        .expect("write output");
}

/// Decode source bytes without destroying what is not UTF-8: valid UTF-8
/// passes through, and each INVALID byte becomes the code point
/// `U+E000 + byte`. [`encode_bytes`] reverses it exactly.
fn decode_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    let mut rest = bytes;
    while !rest.is_empty() {
        match std::str::from_utf8(rest) {
            Ok(s) => {
                out.push_str(s);
                break;
            }
            Err(e) => {
                let good = e.valid_up_to();
                out.push_str(std::str::from_utf8(&rest[..good]).expect("valid prefix"));
                let bad_len = e.error_len().unwrap_or(rest.len() - good).max(1);
                for b in &rest[good..good + bad_len] {
                    out.push(char::from_u32(0xE000 + u32::from(*b)).expect("private use"));
                }
                rest = &rest[good + bad_len..];
            }
        }
    }
    out
}

/// Reverse [`decode_bytes`]: private-use code points go back to their raw
/// byte, everything else is written as UTF-8.
fn encode_bytes(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for ch in s.chars() {
        let cp = u32::from(ch);
        if (0xE000..=0xE0FF).contains(&cp) {
            out.push((cp - 0xE000) as u8);
        } else {
            let mut buf = [0u8; 4];
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        }
    }
    out
}
