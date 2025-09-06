/// Analysis annotations that can be attached to AST nodes
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AstAnnotations {
    /// Bounds information for words
    pub bounds: Option<Bounds>,
    /// Optimization information for loops
    pub loop_analysis: Option<LoopAnalysis>,
    /// Variable usage analysis
    pub variable_analysis: Option<VariableAnalysis>,
}

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

/// Loop optimization analysis
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LoopAnalysis {
    /// Whether the loop variable is used after the loop
    pub variable_used_after: bool,
    /// Whether the variable is overwritten before being used
    pub variable_overwritten_before_use: bool,
    /// Variables that are modified in the loop body
    pub variables_modified_in_loop: Vec<String>,
    /// Variables that are used after the loop ends
    pub variables_used_after_loop: Vec<String>,
}

/// Variable usage analysis
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct VariableAnalysis {
    /// Whether this variable is mutable
    pub is_mutable: bool,
    /// Whether this variable is used after definition
    pub used_after_definition: bool,
    /// Whether this variable is overwritten before use
    pub overwritten_before_use: bool,
}

impl AstAnnotations {
    pub fn new() -> Self {
        AstAnnotations {
            bounds: None,
            loop_analysis: None,
            variable_analysis: None,
        }
    }
    
    pub fn with_bounds(bounds: Bounds) -> Self {
        AstAnnotations {
            bounds: Some(bounds),
            loop_analysis: None,
            variable_analysis: None,
        }
    }
    
    pub fn with_loop_analysis(loop_analysis: LoopAnalysis) -> Self {
        AstAnnotations {
            bounds: None,
            loop_analysis: Some(loop_analysis),
            variable_analysis: None,
        }
    }
    
    pub fn with_variable_analysis(variable_analysis: VariableAnalysis) -> Self {
        AstAnnotations {
            bounds: None,
            loop_analysis: None,
            variable_analysis: Some(variable_analysis),
        }
    }
}

