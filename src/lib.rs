pub mod ast;
pub mod ast_words;
pub mod lexer;
pub mod parser;
// pub mod mir; // TODO: Fix MIR implementation
pub mod ir;
pub mod debug;
pub mod estree;
pub mod shir;
pub mod generator;
pub mod mir_simple;
pub mod shared_utils;
pub mod timeout_manager;
pub mod variable_analysis;
pub mod wasm;

// Only export the main types to avoid conflicts
pub use ast::*;
pub use lexer::{Lexer, Token};
pub use parser::commands::Parser;
pub use parser::utilities::ParserUtilities;
// pub use mir::*; // TODO: Fix MIR implementation
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
