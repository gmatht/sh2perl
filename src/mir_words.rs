use crate::ast_words::*;

/// Conservative bounds for string length and numeric values
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Bounds {
    pub string_length: Option<(usize, usize)>, // (min, max) length
    pub numeric_value: Option<i64>, // if this is a known numeric value
}

impl Bounds {
    pub fn new() -> Self {
        Bounds {
            string_length: None,
            numeric_value: None,
        }
    }
    
    pub fn with_string_length(min: usize, max: usize) -> Self {
        Bounds {
            string_length: Some((min, max)),
            numeric_value: None,
        }
    }
    
    pub fn with_numeric_value(value: i64) -> Self {
        Bounds {
            string_length: None,
            numeric_value: Some(value),
        }
    }
}

/// MIR representation of a word - wraps AST Word with analysis information
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MirWord {
    pub ast_word: Word,
    pub bounds: Bounds,
}

impl MirWord {
    /// Create a MIR word from an AST word with default bounds
    pub fn from_ast_word(ast_word: Word) -> Self {
        MirWord {
            bounds: Bounds::new(),
            ast_word,
        }
    }
    
    /// Create a MIR word with specific bounds
    pub fn with_bounds(ast_word: Word, bounds: Bounds) -> Self {
        MirWord { ast_word, bounds }
    }
    
    /// Get the underlying AST word
    pub fn ast_word(&self) -> &Word {
        &self.ast_word
    }
    
    /// Get the bounds information
    pub fn bounds(&self) -> &Bounds {
        &self.bounds
    }
    
    /// Update the bounds
    pub fn set_bounds(&mut self, bounds: Bounds) {
        self.bounds = bounds;
    }
}

impl std::fmt::Display for MirWord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.ast_word)
    }
}

impl std::ops::Deref for MirWord {
    type Target = Word;
    
    fn deref(&self) -> &Self::Target {
        &self.ast_word
    }
}

impl PartialEq<Word> for MirWord {
    fn eq(&self, other: &Word) -> bool {
        &self.ast_word == other
    }
}

impl PartialEq<str> for MirWord {
    fn eq(&self, other: &str) -> bool {
        self.ast_word == other
    }
}

impl PartialEq<&str> for MirWord {
    fn eq(&self, other: &&str) -> bool {
        self.ast_word == *other
    }
}

impl PartialEq<String> for MirWord {
    fn eq(&self, other: &String) -> bool {
        self.ast_word == other
    }
}

