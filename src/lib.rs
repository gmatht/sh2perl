pub mod ast;
pub mod ast_words;
pub mod lexer;
pub mod parser;

pub mod debug;
pub mod estree;
pub mod ir;
pub mod shir;
pub mod shir_json;
pub mod transforms;

pub mod bc;
pub mod generator;
pub mod shir_json_in;
// Unified backend fleet: the renderers merged from the backend worktrees
// (branch backend/<lang>). Each consumes the ShIR in-process and emits
// <lang> source — one library, every target (PLAN §4 "unified otranspiler").
pub mod c_backend;
pub mod go_backend;
pub mod java_backend;
pub mod js_backend;
pub mod mir_simple;
pub mod perl_backend;
pub mod python_backend;
pub mod rust_backend;
pub mod sh_backend;
pub mod glsl_backend;
pub mod shared_utils;
pub mod shir_passes;
pub mod variable_analysis;
pub mod zig_backend;
// Browser (JS/wasm-bindgen) API — wasm32-unknown-unknown only.
#[cfg(not(target_os = "wasi"))]
pub mod wasm;
// WASI (wasm32-wasip1) library ABI — plain C exports, see wasi_api.rs.
// Feature-gated so the `debashc` command build stays a clean WASI command
// (a module exporting both `_start` and `_initialize` is neither a valid
// command nor a valid reactor in strict runtimes like Node's node:wasi).
#[cfg(all(target_os = "wasi", feature = "wasi-lib"))]
pub mod wasi_api;

// Only export the main types to avoid conflicts
pub use ast::*;
pub use lexer::{Lexer, Token};
pub use parser::commands::Parser;
pub use parser::utilities::ParserUtilities;

pub use generator::Generator;
#[cfg(test)]
pub mod estree_debug_tests {
    use super::*;
    #[test]
    fn dump_test_expr() {
        let cases = [
            "primes=(2)\necho ${primes[@]:0:1}",
            "arr=(a b c)\necho ${arr[@]: -10}",
            "echo ${arr[@]:1:2}",
            "LIST=()\nLIST+=(a b)\nprintf \"%s\" \"${LIST:-}\"",
            "z+=${primes[@]:0:1}",
            "echo ${x:-d}",
        ];
        for c in cases {
            let commands = crate::Parser::new(c).parse().unwrap();
            println!("CASE {c:40} => {:?}", commands);
        }
    }
}
