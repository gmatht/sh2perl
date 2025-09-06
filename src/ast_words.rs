/// Represents parameter expansion operations like ${var:-default}
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ParameterExpansion {
    pub variable: String,
    pub operator: ParameterExpansionOperator,
    pub is_mutable: bool,
}

impl std::fmt::Display for ParameterExpansion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.operator {
            ParameterExpansionOperator::None => write!(f, "${{{}}}", self.variable),
            ParameterExpansionOperator::UppercaseAll => write!(f, "${{{0}^^}}", self.variable),
            ParameterExpansionOperator::LowercaseAll => write!(f, "${{{0},,}}", self.variable),
            ParameterExpansionOperator::UppercaseFirst => write!(f, "${{{0}^}}", self.variable),
            ParameterExpansionOperator::RemoveLongestPrefix(pattern) => write!(f, "${{{0}##{1}}}", self.variable, pattern),
            ParameterExpansionOperator::RemoveShortestPrefix(pattern) => write!(f, "${{{0}#{1}}}", self.variable, pattern),
            ParameterExpansionOperator::RemoveLongestSuffix(pattern) => write!(f, "${{{0}%%{1}}}", self.variable, pattern),
            ParameterExpansionOperator::RemoveShortestSuffix(pattern) => write!(f, "${{{0}%{1}}}", self.variable, pattern),
            ParameterExpansionOperator::SubstituteAll(pattern, replacement) => write!(f, "${{{0}//{1}/{2}}}", self.variable, pattern, replacement),
            ParameterExpansionOperator::DefaultValue(default) => write!(f, "${{{0}:-{1}}}", self.variable, default),
            ParameterExpansionOperator::AssignDefault(default) => write!(f, "${{{0}:={1}}}", self.variable, default),
            ParameterExpansionOperator::ErrorIfUnset(error) => write!(f, "${{{0}:?{1}}}", self.variable, error),
            ParameterExpansionOperator::Basename => write!(f, "${{{0}##*/}}", self.variable),
            ParameterExpansionOperator::Dirname => write!(f, "${{{0}%/*}}", self.variable),
            ParameterExpansionOperator::ArraySlice(offset, length) => {
                if let Some(length_str) = length {
                    write!(f, "${{{}}}:{}:{}", self.variable, offset, length_str)
                } else {
                    write!(f, "${{{}}}:{}", self.variable, offset)
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum ParameterExpansionOperator {
    // No operator (simple variable reference)
    None,
    
    // Case modification
    UppercaseAll,      // ^^
    LowercaseAll,      // ,,
    UppercaseFirst,    // ^
    
    // Substring removal
    RemoveLongestPrefix(String),  // ##pattern
    RemoveShortestPrefix(String), // #pattern
    RemoveLongestSuffix(String),  // %%pattern
    RemoveShortestSuffix(String), // %pattern
    
    // Pattern substitution
    SubstituteAll(String, String), // //pattern/replacement
    
    // Default values
    DefaultValue(String),          // :-default
    AssignDefault(String),         // :=default
    ErrorIfUnset(String),         // :?error
    
    // Path manipulation
    Basename,                      // ##*/
    Dirname,                       // %/*
    
    // Array slice operations
    ArraySlice(String, Option<String>), // :offset or :start:length
}

/// Represents arithmetic expressions
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ArithmeticExpression {
    pub expression: String,
    pub tokens: Vec<ArithmeticToken>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum ArithmeticToken {
    Number(String),
    Variable(String),
    Operator(String),
    ParenOpen,
    ParenClose,
}

/// Represents brace expansion like {a,b,c} or {1..10}
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BraceExpansion {
    pub prefix: Option<String>,
    pub items: Vec<BraceItem>,
    pub suffix: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[allow(dead_code)]
pub enum BraceItem {
    Literal(String),
    Range(BraceRange),
    Sequence(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BraceRange {
    pub start: String,
    pub end: String,
    pub step: Option<String>,
    pub format: Option<String>,
}

/// Represents string interpolation with embedded variables and expansions
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct StringInterpolation {
    pub parts: Vec<StringPart>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[allow(dead_code)]
pub enum StringPart {
    Literal(String),
    Variable(String),
    ParameterExpansion(ParameterExpansion),
    MapAccess(String, String), // map_name, key
    MapKeys(String), // !map[@] -> get keys of associative array
    MapLength(String), // #arr[@] -> get length of array
    ArraySlice(String, String, Option<String>), // array_name, offset, optional_length
    Arithmetic(ArithmeticExpression),
    CommandSubstitution(Box<crate::ast::Command>),
}

/// Represents a word in the shell language (literal, variable, expansion, etc.)
/// This is the AST version - pure syntax without analysis information
/// The Option<()> field is reserved for future MIR annotations
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum Word {
    Literal(String, Option<()>),
    Variable(String, bool, Option<()>), // variable_name, is_mutable, annotations
    ParameterExpansion(ParameterExpansion, Option<()>),
    Array(String, Vec<String>, Option<()>), // array_name, elements, annotations
    MapAccess(String, String, Option<()>), // map_name, key, annotations
    MapKeys(String, Option<()>), // !map[@] -> get keys of associative array, annotations
    MapLength(String, Option<()>), // #arr[@] -> get length of array, annotations
    ArraySlice(String, String, Option<String>, Option<()>), // array_name, offset, optional_length, annotations
    Arithmetic(ArithmeticExpression, Option<()>),
    BraceExpansion(BraceExpansion, Option<()>),
    CommandSubstitution(Box<crate::ast::Command>, Option<()>),
    StringInterpolation(StringInterpolation, Option<()>),
}

impl std::fmt::Display for Word {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Word::Literal(s, _) => write!(f, "{}", s),
            Word::Variable(var, _, _) => write!(f, "${}", var),
            Word::ParameterExpansion(pe, _) => {
                match &pe.operator {
                    ParameterExpansionOperator::None => write!(f, "${{{}}}", pe.variable),
                    ParameterExpansionOperator::UppercaseAll => write!(f, "${{{}}}", pe.variable),
                    ParameterExpansionOperator::LowercaseAll => write!(f, "${{{}}}", pe.variable),
                    ParameterExpansionOperator::UppercaseFirst => write!(f, "${{{}}}", pe.variable),
                    ParameterExpansionOperator::RemoveLongestPrefix(pattern) => write!(f, "${{{}}}##{}", pe.variable, pattern),
                    ParameterExpansionOperator::RemoveShortestPrefix(pattern) => write!(f, "${{{}}}#{}", pe.variable, pattern),
                    ParameterExpansionOperator::RemoveLongestSuffix(pattern) => write!(f, "${{{}}}%%{}", pe.variable, pattern),
                    ParameterExpansionOperator::RemoveShortestSuffix(pattern) => write!(f, "${{{}}}%{}", pe.variable, pattern),
                    ParameterExpansionOperator::SubstituteAll(pattern, replacement) => write!(f, "${{{}}}//{}/{}", pe.variable, pattern, replacement),
                    ParameterExpansionOperator::DefaultValue(default) => write!(f, "${{{}}}:-{}", pe.variable, default),
                    ParameterExpansionOperator::AssignDefault(default) => write!(f, "${{{}}}:={}", pe.variable, default),
                    ParameterExpansionOperator::ErrorIfUnset(error) => write!(f, "${{{}}}:?{}", pe.variable, error),
                    ParameterExpansionOperator::Basename => write!(f, "${{{}}}##*/", pe.variable),
                    ParameterExpansionOperator::Dirname => write!(f, "${{{}}}%/*", pe.variable),
                    ParameterExpansionOperator::ArraySlice(offset, length) => {
                        if let Some(length_str) = length {
                            write!(f, "${{{}}}:{}:{}", pe.variable, offset, length_str)
                        } else {
                            write!(f, "${{{}}}:{}", pe.variable, offset)
                        }
                    }
                }
            },
            Word::Array(name, elements, _) => write!(f, "{}=({})", name, elements.join(" ")),
            Word::MapAccess(map_name, key, _) => write!(f, "{}[{}]", map_name, key),
            Word::MapKeys(map_name, _) => write!(f, "!{}[@]", map_name),
            Word::MapLength(map_name, _) => write!(f, "#{}[@]", map_name),
            Word::ArraySlice(array_name, offset, length, _) => {
                if let Some(length_str) = length {
                    write!(f, "${{{}}}[@]:{}:{}", array_name, offset, length_str)
                } else {
                    write!(f, "${{{}}}[@]:{}", array_name, offset)
                }
            },
            Word::Arithmetic(expr, _) => write!(f, "{}", expr.expression),
            Word::BraceExpansion(expansion, _) => {
                let mut result = String::new();
                if let Some(ref prefix) = expansion.prefix {
                    result.push_str(prefix);
                }
                for (i, item) in expansion.items.iter().enumerate() {
                    if i > 0 {
                        result.push(',');
                    }
                    match item {
                        BraceItem::Literal(s) => result.push_str(s),
                        BraceItem::Range(range) => {
                            result.push_str(&range.start);
                            result.push_str("..");
                            result.push_str(&range.end);
                            if let Some(ref step) = range.step {
                                result.push_str("..");
                                result.push_str(step);
                            }
                        }
                        BraceItem::Sequence(seq) => {
                            result.push_str(&seq.join(","));
                        }
                    }
                }
                if let Some(ref suffix) = expansion.suffix {
                    result.push_str(suffix);
                }
                write!(f, "{{{}}}", result)
            }
            Word::CommandSubstitution(_, _) => write!(f, "$(...)"),
            Word::StringInterpolation(interp, _) => {
                let mut result = String::new();
                for part in &interp.parts {
                    match part {
                        StringPart::Literal(s) => result.push_str(s),
                        StringPart::Variable(var) => result.push_str(&format!("${}", var)),
                        StringPart::ParameterExpansion(pe) => {
                            match &pe.operator {
                                ParameterExpansionOperator::None => result.push_str(&format!("${{{}}}", pe.variable)),
                                ParameterExpansionOperator::UppercaseAll => result.push_str(&format!("${{{}}}", pe.variable)),
                                ParameterExpansionOperator::LowercaseAll => result.push_str(&format!("${{{}}}", pe.variable)),
                                ParameterExpansionOperator::UppercaseFirst => result.push_str(&format!("${{{}}}", pe.variable)),
                                ParameterExpansionOperator::RemoveLongestPrefix(pattern) => result.push_str(&format!("${{{}}}##{}", pe.variable, pattern)),
                                ParameterExpansionOperator::RemoveShortestPrefix(pattern) => result.push_str(&format!("${{{}}}#{}", pe.variable, pattern)),
                                ParameterExpansionOperator::RemoveLongestSuffix(pattern) => result.push_str(&format!("${{{}}}%%{}", pe.variable, pattern)),
                                ParameterExpansionOperator::RemoveShortestSuffix(pattern) => result.push_str(&format!("${{{}}}%{}", pe.variable, pattern)),
                                ParameterExpansionOperator::SubstituteAll(pattern, replacement) => result.push_str(&format!("${{{}}}//{}/{}", pe.variable, pattern, replacement)),
                                ParameterExpansionOperator::DefaultValue(default) => result.push_str(&format!("${{{}}}:-{}", pe.variable, default)),
                                ParameterExpansionOperator::AssignDefault(default) => result.push_str(&format!("${{{}}}:={}", pe.variable, default)),
                                ParameterExpansionOperator::ErrorIfUnset(error) => result.push_str(&format!("${{{}}}:?{}", pe.variable, error)),
                                ParameterExpansionOperator::Basename => result.push_str(&format!("${{{}}}##*/", pe.variable)),
                                ParameterExpansionOperator::Dirname => result.push_str(&format!("${{{}}}%/*", pe.variable)),
                                ParameterExpansionOperator::ArraySlice(offset, length) => {
                                    if let Some(length_str) = length {
                                        result.push_str(&format!("${{{}}}:{1}:{2}", pe.variable, offset, length_str))
                                    } else {
                                        result.push_str(&format!("${{{}}}:{1}", pe.variable, offset))
                                    }
                                }
                            }
                        }
                        StringPart::MapAccess(map_name, key) => result.push_str(&format!("{}[{}]", map_name, key)),
                        StringPart::MapKeys(map_name) => result.push_str(&format!("!{}[@]", map_name)),
                        StringPart::MapLength(map_name) => result.push_str(&format!("#{}[@]", map_name)),
                        StringPart::ArraySlice(array_name, offset, length) => {
                            if let Some(length_str) = length {
                                result.push_str(&format!("${{{}[@]}}:{}:{}", array_name, offset, length_str));
                            } else {
                                result.push_str(&format!("${{{}[@]}}:{}", array_name, offset));
                            }
                        },
                        StringPart::Arithmetic(expr) => result.push_str(&expr.expression),
                        StringPart::CommandSubstitution(_) => result.push_str("$(...)"),
                    }
                }
                write!(f, "{}", result)
            }
        }
    }
}

impl Word {
    /// Create a literal word
    pub fn literal(s: String) -> Self {
        Word::Literal(s, None)
    }
    
    /// Create a variable word
    pub fn variable(name: String) -> Self {
        Word::Variable(name, false, None)
    }
    
    /// Create a parameter expansion word
    pub fn parameter_expansion(pe: ParameterExpansion) -> Self {
        Word::ParameterExpansion(pe, None)
    }
    
    /// Create an array word
    pub fn array(name: String, elements: Vec<String>) -> Self {
        Word::Array(name, elements, None)
    }
    
    /// Create a map access word
    pub fn map_access(map_name: String, key: String) -> Self {
        Word::MapAccess(map_name, key, None)
    }
    
    /// Create a map keys word
    pub fn map_keys(map_name: String) -> Self {
        Word::MapKeys(map_name, None)
    }
    
    /// Create a map length word
    pub fn map_length(map_name: String) -> Self {
        Word::MapLength(map_name, None)
    }
    
    /// Create an array slice word
    pub fn array_slice(array_name: String, offset: String, length: Option<String>) -> Self {
        Word::ArraySlice(array_name, offset, length, None)
    }
    
    /// Create an arithmetic word
    pub fn arithmetic(expr: ArithmeticExpression) -> Self {
        Word::Arithmetic(expr, None)
    }
    
    /// Create a brace expansion word
    pub fn brace_expansion(expansion: BraceExpansion) -> Self {
        Word::BraceExpansion(expansion, None)
    }
    
    /// Create a command substitution word
    pub fn command_substitution(cmd: Box<crate::ast::Command>) -> Self {
        Word::CommandSubstitution(cmd, None)
    }
    
    /// Create a string interpolation word
    pub fn string_interpolation(interp: StringInterpolation) -> Self {
        Word::StringInterpolation(interp, None)
    }


    
    /// Get a string representation of the word, suitable for display
    pub fn to_string(&self) -> String {
        match self {
            Word::Literal(s, _) => s.to_string(),
            Word::Variable(var, _, _) => format!("${}", var),
            Word::ParameterExpansion(pe, _) => {
                match &pe.operator {
                    ParameterExpansionOperator::None => format!("${{{}}}", pe.variable),
                    ParameterExpansionOperator::UppercaseAll => format!("${{{}}}", pe.variable),
                    ParameterExpansionOperator::LowercaseAll => format!("${{{}}}", pe.variable),
                    ParameterExpansionOperator::UppercaseFirst => format!("${{{}}}", pe.variable),
                    ParameterExpansionOperator::RemoveLongestPrefix(pattern) => format!("${{{}}}##{}", pe.variable, pattern),
                    ParameterExpansionOperator::RemoveShortestPrefix(pattern) => format!("${{{}}}#{}", pe.variable, pattern),
                    ParameterExpansionOperator::RemoveLongestSuffix(pattern) => format!("${{{}}}%%{}", pe.variable, pattern),
                    ParameterExpansionOperator::RemoveShortestSuffix(pattern) => format!("${{{}}}%{}", pe.variable, pattern),
                    ParameterExpansionOperator::SubstituteAll(pattern, replacement) => format!("${{{}}}//{}/{}", pe.variable, pattern, replacement),
                    ParameterExpansionOperator::DefaultValue(default) => format!("${{{}}}:-{}", pe.variable, default),
                    ParameterExpansionOperator::AssignDefault(default) => format!("${{{}}}:={}", pe.variable, default),
                    ParameterExpansionOperator::ErrorIfUnset(error) => format!("${{{}}}:?{}", pe.variable, error),
                    ParameterExpansionOperator::Basename => format!("${{{}}}##*/", pe.variable),
                    ParameterExpansionOperator::Dirname => format!("${{{}}}%/*", pe.variable),
                    ParameterExpansionOperator::ArraySlice(offset, length) => {
                        if let Some(length_str) = length {
                            format!("${{{}}}:{1}:{2}", pe.variable, offset, length_str)
                        } else {
                            format!("${{{}}}:{1}", pe.variable, offset)
                        }
                    }
                }
            },
            Word::Array(name, elements, _) => format!("{}=({})", name, elements.join(" ")),
            Word::MapAccess(map_name, key, _) => format!("{}[{}]", map_name, key),
            Word::MapKeys(map_name, _) => format!("!{}[@]", map_name),
            Word::MapLength(map_name, _) => format!("#{}[@]", map_name),
            Word::ArraySlice(array_name, offset, length, _) => {
                if let Some(length_str) = length {
                    format!("${{{}}}[@]:{}:{}", array_name, offset, length_str)
                } else {
                    format!("${{{}}}[@]:{}", array_name, offset)
                }
            },
            Word::Arithmetic(expr, _) => expr.expression.to_string(),
            Word::BraceExpansion(expansion, _) => {
                let mut result = String::new();
                if let Some(ref prefix) = expansion.prefix {
                    result.push_str(prefix);
                }
                for (i, item) in expansion.items.iter().enumerate() {
                    if i > 0 {
                        result.push(',');
                    }
                    match item {
                        BraceItem::Literal(s) => result.push_str(s),
                        BraceItem::Range(range) => {
                            result.push_str(&range.start);
                            result.push_str("..");
                            result.push_str(&range.end);
                            if let Some(ref step) = range.step {
                                result.push_str("..");
                                result.push_str(step);
                            }
                        }
                        BraceItem::Sequence(seq) => {
                            result.push_str(&seq.join(","));
                        }
                    }
                }
                if let Some(ref suffix) = expansion.suffix {
                    result.push_str(suffix);
                }
                format!("{{{}}}", result)
            }
            Word::CommandSubstitution(_, _) => "$(...)".to_string(),
            Word::StringInterpolation(interp, _) => {
                let mut result = String::new();
                for part in &interp.parts {
                    match part {
                        StringPart::Literal(s) => result.push_str(s),
                        StringPart::Variable(var) => result.push_str(&format!("${}", var)),
                        StringPart::ParameterExpansion(pe) => {
                            match &pe.operator {
                                ParameterExpansionOperator::None => result.push_str(&format!("${{{}}}", pe.variable)),
                                ParameterExpansionOperator::UppercaseAll => result.push_str(&format!("${{{}}}", pe.variable)),
                                ParameterExpansionOperator::LowercaseAll => result.push_str(&format!("${{{}}}", pe.variable)),
                                ParameterExpansionOperator::UppercaseFirst => result.push_str(&format!("${{{}}}", pe.variable)),
                                ParameterExpansionOperator::RemoveLongestPrefix(pattern) => result.push_str(&format!("${{{}}}##{}", pe.variable, pattern)),
                                ParameterExpansionOperator::RemoveShortestPrefix(pattern) => result.push_str(&format!("${{{}}}#{}", pe.variable, pattern)),
                                ParameterExpansionOperator::RemoveLongestSuffix(pattern) => result.push_str(&format!("${{{}}}%%{}", pe.variable, pattern)),
                                ParameterExpansionOperator::RemoveShortestSuffix(pattern) => result.push_str(&format!("${{{}}}%{}", pe.variable, pattern)),
                                ParameterExpansionOperator::SubstituteAll(pattern, replacement) => result.push_str(&format!("${{{}}}//{}/{}", pe.variable, pattern, replacement)),
                                ParameterExpansionOperator::DefaultValue(default) => result.push_str(&format!("${{{}}}:-{}", pe.variable, default)),
                                ParameterExpansionOperator::AssignDefault(default) => result.push_str(&format!("${{{}}}:={}", pe.variable, default)),
                                ParameterExpansionOperator::ErrorIfUnset(error) => result.push_str(&format!("${{{}}}:?{}", pe.variable, error)),
                                ParameterExpansionOperator::Basename => result.push_str(&format!("${{{}}}##*/", pe.variable)),
                                ParameterExpansionOperator::Dirname => result.push_str(&format!("${{{}}}%/*", pe.variable)),
                                ParameterExpansionOperator::ArraySlice(offset, length) => {
                                    if let Some(length_str) = length {
                                        result.push_str(&format!("${{{}}}:{1}:{2}", pe.variable, offset, length_str))
                                    } else {
                                        result.push_str(&format!("${{{}}}:{1}", pe.variable, offset))
                                    }
                                }
                            }
                        },
                        StringPart::MapAccess(map_name, key) => result.push_str(&format!("${{{}}}[{}]", map_name, key)),
                        StringPart::MapKeys(map_name) => result.push_str(&format!("${{!{}}}[@]", map_name)),
                        StringPart::MapLength(map_name) => result.push_str(&format!("${{#{}}}[@]", map_name)),
                        StringPart::ArraySlice(array_name, offset, length) => {
                            if let Some(length_str) = length {
                                result.push_str(&format!("${{{}[@]}}:{}:{}", array_name, offset, length_str));
                            } else {
                                result.push_str(&format!("${{{}[@]}}:{}", array_name, offset));
                            }
                        },
                        StringPart::Arithmetic(expr) => result.push_str(&expr.expression),
                        StringPart::CommandSubstitution(_) => result.push_str("$(...)"),
                    }
                }
                format!("\"{}\"", result)
            }
        }
    }

    /// Get the raw string value if this is a literal, or convert to string otherwise
    pub fn as_str(&self) -> &str {
        match self {
            Word::Literal(s, _) => s,
            _ => "",
        }
    }

    /// Get the literal value if this word is a literal, None otherwise
    pub fn as_literal(&self) -> Option<&str> {
        match self {
            Word::Literal(s, _) => Some(s),
            _ => None,
        }
    }
}

impl std::ops::Deref for Word {
    type Target = str;
    
    fn deref(&self) -> &Self::Target {
        match self {
            Word::Literal(s, _) => s,
            _ => "",
        }
    }
}

impl PartialEq<str> for Word {
    fn eq(&self, other: &str) -> bool {
        match self {
            Word::Literal(s, _) => s == other,
            Word::Variable(var, _, _) => var == other,
            Word::ParameterExpansion(pe, _) => pe.variable == other,
            Word::MapKeys(map_name, _) => map_name == other,
            Word::MapLength(map_name, _) => map_name == other,
            Word::Arithmetic(expr, _) => expr.expression == other,
            _ => false,
        }
    }
}

impl PartialEq<&str> for Word {
    fn eq(&self, other: &&str) -> bool {
        self == *other
    }
}

impl PartialEq<String> for Word {
    fn eq(&self, other: &String) -> bool {
        self == other.as_str()
    }
}