pub mod ast;
pub mod ast_words;
pub mod lexer;
pub mod parser;
// pub mod mir; // TODO: Fix MIR implementation
pub mod debug;
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
