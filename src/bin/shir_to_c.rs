//! DRAFT: ShIR JSON -> C renderer (universal backend contract, path C).
//!
//! Consumes `debashc file --shir foo.sh` output on stdin and emits C,
//! via the worktree's library renderer (`c_backend::shir_to_c`) after a
//! ShIR-JSON round-trip (`shir_json_in::shir_json_to_ir`).
//!
//! This is the contract path for static/heterogeneous targets
//! (docs/backend-universal-contract.md): ShIR JSON is the
//! language-neutral serialized IR (A1) with type verdicts (A2) and
//! purity (A3); the ESTree JSON stays the JS-runtime's contract ("wrong
//! shape for everyone else"). `estree_to_c` (the old ESTree-JSON
//! consumer) is retired — a C renderer must not reverse-engineer JS
//! idioms (`process.stdout.write`, `Math.trunc`, `String(...)`) out of
//! a JS rendering.
//!
//! Usage:
//!   debashc file --shir foo.sh | cargo run --bin shir_to_c > foo.c \
//!     && gcc foo.c -lm -o foo && ./foo

use std::io::{self, Read};

fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).expect("read stdin");
    let prog = match debashl::shir_json_in::shir_json_to_ir(&input) {
        Ok(p) => p,
        Err(e) => {
            // empty/non-JSON input (parse-broken scripts emit no ShIR)
            eprintln!("shir_to_c: ShIR JSON ingress: {e}");
            std::process::exit(1);
        }
    };
    print!("{}", debashl::c_backend::shir_to_c(&prog));
}
