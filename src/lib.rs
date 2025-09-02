pub mod lexer;
pub mod parser;
pub mod ast;
pub mod mir;
pub mod debug;
pub mod shared_utils;
pub mod generator;
pub mod variable_analysis;
pub mod wasm;

// Only export the main types to avoid conflicts
pub use lexer::{Lexer, Token};
pub use parser::commands::Parser;
pub use parser::utilities::ParserUtilities;
pub use ast::*;
pub use mir::*;
pub use generator::Generator;
