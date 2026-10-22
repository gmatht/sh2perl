#![allow(unreachable_patterns)]
#![allow(unused_assignments)]
#![allow(dead_code)]
#![allow(unexpected_cfgs)]

/// Debug traces from the shell parser/renderer, normally OFF.
///
/// These used to be unconditional `eprintln!`s, so ordinary constructs —
/// `${1:-5}`, `a[0]`, `declare -A m` — wrote `DEBUG ...` lines to stderr;
/// a single `bash-O4` parse of MIMEcroft_fb.sh wrote **176 lines** of
/// `DEBUG parse_variable_expansion: …` / `DBG store_ref(…)` debris. That
/// buries a real error in noise and makes any stderr assertion in a test
/// or gate meaningless. Set `SH2_DEBUG_PARSER=1` to get them back (the
/// same env-gate convention `SH2_DEBUG_DEMOTE` already uses in
/// `c_backend.rs`).
///
/// Defined before the `mod` declarations so textual macro scoping makes it
/// available throughout the crate without touching the public surface.
macro_rules! pdebug {
    ($($arg:tt)*) => {
        if std::env::var_os("SH2_DEBUG_PARSER").is_some() {
            eprintln!($($arg)*);
        }
    };
}

pub mod ast;
pub mod ast_words;
pub mod lexer;
pub mod parser;

pub mod debug;
pub mod estree;
pub mod ir;
pub mod pipeline_native;
pub mod shir;
pub mod naming;
pub mod render_ext;
pub mod render_ext_expr;
pub mod render_ext_estree;
pub mod shir_nodes;
pub mod shir_json;
pub mod transforms;

pub mod bc;
pub mod perl_backend; // worktree-local: Perl backend renderer (branch backend/perl)
pub mod perl_generator_fixed; // worktree-local: post-processor wrapping ir::shir_to_perl with targeted fixes
pub mod shir_json_in;
// Unified backend fleet: the renderers merged from the backend worktrees
// (branch backend/<lang>). Each consumes the ShIR in-process and emits
// <lang> source — one library, every target (PLAN §4 "unified otranspiler").
pub mod c_backend;
pub mod c_profile;
pub mod go_backend;
pub mod java_backend;
pub mod mir_simple;
pub mod python_backend;
pub mod rust_backend;
pub mod sh_backend;
pub mod glsl_backend;
pub mod vulkan_backend;
pub mod cuda_backend; // compute-shader emitter for bash-O4 autoshaderise (docs/BASH-O4.md §4.4)
pub mod shared_utils;
pub mod shir_passes;
pub mod variable_analysis;
pub mod zig_backend;
pub mod lint_backend; // lint/diagnostic backend (renders analysis verdicts, not code)
// Browser (JS/wasm-bindgen) API — wasm32 only (wasm-bindgen links the
// wasm import shim; ungated it rides along on native/win32 builds).
#[cfg(target_arch = "wasm32")]
pub mod wasm;
// WASI (wasm32-wasip1) library ABI — plain C exports, see wasi_api.rs.
// Feature-gated so a WASI command build (the otranspilerl-cli bin) stays a
// (a module exporting both `_start` and `_initialize` is neither a valid
// command nor a valid reactor in strict runtimes like Node's node:wasi).
#[cfg(all(target_os = "wasi", feature = "wasi-lib"))]
pub mod wasi_api;

// Only export the main types to avoid conflicts
pub use ast::*;
pub use lexer::{Lexer, Token};
pub use parser::commands::Parser;
pub use parser::utilities::ParserUtilities;

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
