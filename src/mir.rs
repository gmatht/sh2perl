use crate::ast::Command;

/// Conservative bounds for string length and numeric values
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Bounds {
    /// String length bounds (min, max)
    pub string_length: Option<(usize, usize)>,
    /// Numeric value bounds (min, max)
    pub numeric_value: Option<(i64, i64)>,
}

impl Bounds {
    pub fn unknown() -> Self {
        Self {
            string_length: None,
            numeric_value: None,
        }
    }
    
    pub fn string_length(min: usize, max: usize) -> Self {
        Self {
            string_length: Some((min, max)),
            numeric_value: None,
        }
    }
    
    pub fn numeric_value(min: i64, max: i64) -> Self {
        Self {
            string_length: None,
            numeric_value: Some((min, max)),
        }
    }
    
    pub fn both_string_and_numeric(string_min: usize, string_max: usize, numeric_min: i64, numeric_max: i64) -> Self {
        Self {
            string_length: Some((string_min, string_max)),
            numeric_value: Some((numeric_min, numeric_max)),
        }
    }
}

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

/// Represents a word in the shell language (literal, variable, expansion, etc.)
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum Word {
    Literal(String, Bounds),
    Variable(String, Bounds, bool), // variable_name, bounds, is_mutable
    ParameterExpansion(ParameterExpansion, Bounds),
    Array(String, Vec<String>, Bounds), // array_name, elements, bounds
    MapAccess(String, String, Bounds), // map_name, key, bounds
    MapKeys(String, Bounds), // !map[@] -> get keys of associative array, bounds
    MapLength(String, Bounds), // #arr[@] -> get length of array, bounds
    ArraySlice(String, String, Option<String>, Bounds), // array_name, offset, optional_length, bounds
    Arithmetic(ArithmeticExpression, Bounds),
    BraceExpansion(BraceExpansion, Bounds),
    CommandSubstitution(Box<Command>, Bounds),
    StringInterpolation(StringInterpolation, Bounds),
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
                                ParameterExpansionOperator::ArraySlice(var, offset) => {
                                    if let Some(offset_str) = offset {
                                        result.push_str(&format!("${{{}}}:{1}:{2}", pe.variable, var, offset_str))
                                    } else {
                                        result.push_str(&format!("${{{}}}:{1}", pe.variable, var))
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
    /// Create a literal word with known string length bounds
    pub fn literal(s: String) -> Self {
        let len = s.len();
        Word::Literal(s, Bounds::string_length(len, len))
    }
    
    /// Create a variable word with unknown bounds
    pub fn variable(name: String) -> Self {
        Word::Variable(name, Bounds::unknown(), false)
    }
    
    /// Create a parameter expansion word with unknown bounds
    pub fn parameter_expansion(pe: ParameterExpansion) -> Self {
        Word::ParameterExpansion(pe, Bounds::unknown())
    }
    
    /// Create an array word with known bounds based on elements
    pub fn array(name: String, elements: Vec<String>) -> Self {
        let total_length = elements.iter().map(|s| s.len()).sum::<usize>() + elements.len().saturating_sub(1); // + spaces between
        let max_element_length = elements.iter().map(|s| s.len()).max().unwrap_or(0);
        let bounds = if elements.is_empty() {
            Bounds::string_length(0, 0)
        } else {
            Bounds::string_length(max_element_length, total_length)
        };
        Word::Array(name, elements, bounds)
    }
    
    /// Create a map access word with unknown bounds
    pub fn map_access(map_name: String, key: String) -> Self {
        Word::MapAccess(map_name, key, Bounds::unknown())
    }
    
    /// Create a map keys word with unknown bounds
    pub fn map_keys(map_name: String) -> Self {
        Word::MapKeys(map_name, Bounds::unknown())
    }
    
    /// Create a map length word with numeric bounds (0 to max array size)
    pub fn map_length(map_name: String) -> Self {
        Word::MapLength(map_name, Bounds::numeric_value(0, i64::MAX))
    }
    
    /// Create an array slice word with unknown bounds
    pub fn array_slice(array_name: String, offset: String, length: Option<String>) -> Self {
        Word::ArraySlice(array_name, offset, length, Bounds::unknown())
    }
    
    /// Create an arithmetic word with unknown bounds
    pub fn arithmetic(expr: ArithmeticExpression) -> Self {
        Word::Arithmetic(expr, Bounds::unknown())
    }
    
    /// Create a brace expansion word with bounds based on expansion
    pub fn brace_expansion(expansion: BraceExpansion) -> Self {
        // Calculate conservative bounds for brace expansion
        let mut min_length = 0;
        let mut max_length = 0;
        
        for item in &expansion.items {
            match item {
                BraceItem::Literal(s) => {
                    let len = s.len();
                    min_length = min_length.max(len);
                    max_length += len;
                }
                BraceItem::Range(range) => {
                    // For ranges, estimate bounds
                    let estimated_min = 1; // minimum single character
                    let estimated_max = 10; // conservative estimate
                    min_length = min_length.max(estimated_min);
                    max_length += estimated_max;
                }
                BraceItem::Sequence(seq) => {
                    // For sequences, calculate bounds based on items
                    for item in seq {
                        let len = item.len();
                        min_length = min_length.max(len);
                        max_length += len;
                    }
                }
            }
        }
        
        // Account for prefix and suffix
        if let Some(prefix) = &expansion.prefix {
            let prefix_len = prefix.len();
            min_length += prefix_len;
            max_length += prefix_len;
        }
        if let Some(suffix) = &expansion.suffix {
            let suffix_len = suffix.len();
            min_length += suffix_len;
            max_length += suffix_len;
        }
        
        Word::BraceExpansion(expansion, Bounds::string_length(min_length, max_length))
    }
    
    /// Create a command substitution word with unknown bounds
    pub fn command_substitution(cmd: Box<Command>) -> Self {
        Word::CommandSubstitution(cmd, Bounds::unknown())
    }
    
    /// Create a string interpolation word with bounds based on parts
    pub fn string_interpolation(interp: StringInterpolation) -> Self {
        let mut min_length = 0;
        let mut max_length = 0;
        
        for part in &interp.parts {
            match part {
                StringPart::Literal(s) => {
                    let len = s.len();
                    min_length += len;
                    max_length += len;
                }
                _ => {
                    // For variable expansions, we don't know the bounds
                    min_length += 0;
                    max_length += 100; // conservative estimate
                }
            }
        }
        
        Word::StringInterpolation(interp, Bounds::string_length(min_length, max_length))
    }
    
    /// Get the bounds for this word
    pub fn bounds(&self) -> &Bounds {
        match self {
            Word::Literal(_, bounds) => bounds,
            Word::Variable(_, bounds, _) => bounds,
            Word::ParameterExpansion(_, bounds) => bounds,
            Word::Array(_, _, bounds) => bounds,
            Word::MapAccess(_, _, bounds) => bounds,
            Word::MapKeys(_, bounds) => bounds,
            Word::MapLength(_, bounds) => bounds,
            Word::ArraySlice(_, _, _, bounds) => bounds,
            Word::Arithmetic(_, bounds) => bounds,
            Word::BraceExpansion(_, bounds) => bounds,
            Word::CommandSubstitution(_, bounds) => bounds,
            Word::StringInterpolation(_, bounds) => bounds,
        }
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
    CommandSubstitution(Box<Command>),
}

/// MIR representation of a for loop with optimization information
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MirForLoop {
    pub variable: String,
    pub items: Vec<Word>,
    pub body: Vec<Command>,
    pub variable_used_after: bool,  // Whether the loop variable is used after the loop
    pub variable_overwritten_before_use: bool,  // Whether the variable is overwritten before being used
}

/// MIR representation of a while loop with optimization information
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MirWhileLoop {
    pub condition: Box<Command>,
    pub body: Vec<Command>,
    pub variables_modified_in_loop: Vec<String>,  // Variables that are modified in the loop body
    pub variables_used_after_loop: Vec<String>,  // Variables that are used after the loop ends
}

/// MIR representation of commands with optimization information
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum MirCommand {
    Simple(SimpleCommand),
    Pipeline(Pipeline),
    Redirect(RedirectCommand),
    And(Box<MirCommand>, Box<MirCommand>),
    Or(Box<MirCommand>, Box<MirCommand>),
    For(MirForLoop),
    While(MirWhileLoop),
    If(IfStatement),
    Case(CaseStatement),
    Function(Function),
    Subshell(Box<MirCommand>),
    Background(Box<MirCommand>),
}

/// Import the necessary types from AST
use crate::ast::{SimpleCommand, Pipeline, RedirectCommand, IfStatement, CaseStatement, Function, ForLoop, WhileLoop};

impl MirCommand {
    /// Expand a brace expansion into a list of literal words
    fn expand_brace_expansion(expansion: &BraceExpansion) -> Vec<Word> {
        let mut expanded_words = Vec::new();
        let prefix = expansion.prefix.as_deref().unwrap_or("");
        let suffix = expansion.suffix.as_deref().unwrap_or("");
        
        if expansion.items.len() == 1 {
            // Single item expansion
            let item = &expansion.items[0];
            match item {
                BraceItem::Literal(s) => {
                    expanded_words.push(Word::literal(format!("{}{}{}", prefix, s, suffix)));
                }
                BraceItem::Range(range) => {
                    // Handle numeric ranges like {1..5}
                    if let (Ok(start_num), Ok(end_num)) = (range.start.parse::<i32>(), range.end.parse::<i32>()) {
                        let step = range.step.as_ref().and_then(|s| s.parse::<i32>().ok()).unwrap_or(1);
                        let mut current = start_num;
                        while if step > 0 { current <= end_num } else { current >= end_num } {
                            expanded_words.push(Word::literal(format!("{}{}{}", prefix, current, suffix)));
                            current += step;
                        }
                    } else {
                        // Handle character ranges like {a..c}
                        if let (Some(start_char), Some(end_char)) = (range.start.chars().next(), range.end.chars().next()) {
                            let step = range.step.as_ref().and_then(|s| s.parse::<i32>().ok()).unwrap_or(1);
                            let mut current = start_char as i32;
                            let end_code = end_char as i32;
                            while if step > 0 { current <= end_code } else { current >= end_code } {
                                if let Some(c) = char::from_u32(current as u32) {
                                    expanded_words.push(Word::literal(format!("{}{}{}", prefix, c, suffix)));
                                }
                                current += step;
                            }
                        }
                    }
                }
                BraceItem::Sequence(seq) => {
                    // Handle sequence items like {one,two,three}
                    for item in seq {
                        expanded_words.push(Word::literal(format!("{}{}{}", prefix, item, suffix)));
                    }
                }
            }
        } else {
            // Multiple items - check if this is a sequence (all literals) or cartesian product
            let all_literals = expansion.items.iter().all(|item| matches!(item, BraceItem::Literal(_)));
            
            if all_literals {
                // This is a sequence like {a,b,c} - just expand each literal
                for item in &expansion.items {
                    if let BraceItem::Literal(s) = item {
                        expanded_words.push(Word::literal(format!("{}{}{}", prefix, s, suffix)));
                    }
                }
            } else {
                // Multiple items with ranges - generate cartesian product
                let mut expanded_items: Vec<Vec<String>> = Vec::new();
                for item in &expansion.items {
                    let mut item_expansions = Vec::new();
                    match item {
                        BraceItem::Literal(s) => {
                            item_expansions.push(s.clone());
                        }
                        BraceItem::Range(range) => {
                            // Handle numeric ranges
                            if let (Ok(start_num), Ok(end_num)) = (range.start.parse::<i32>(), range.end.parse::<i32>()) {
                                let step = range.step.as_ref().and_then(|s| s.parse::<i32>().ok()).unwrap_or(1);
                                let mut current = start_num;
                                while if step > 0 { current <= end_num } else { current >= end_num } {
                                    item_expansions.push(current.to_string());
                                    current += step;
                                }
                            } else {
                                // Handle character ranges
                                if let (Some(start_char), Some(end_char)) = (range.start.chars().next(), range.end.chars().next()) {
                                    let step = range.step.as_ref().and_then(|s| s.parse::<i32>().ok()).unwrap_or(1);
                                    let mut current = start_char as i32;
                                    let end_code = end_char as i32;
                                    while if step > 0 { current <= end_code } else { current >= end_code } {
                                        if let Some(c) = char::from_u32(current as u32) {
                                            item_expansions.push(c.to_string());
                                        }
                                        current += step;
                                    }
                                }
                            }
                        }
                        BraceItem::Sequence(seq) => {
                            item_expansions.extend(seq.clone());
                        }
                    }
                    expanded_items.push(item_expansions);
                }
                
                // Generate cartesian product
                let cartesian = Self::generate_cartesian_product(&expanded_items);
                
                // Add prefix and suffix to each item
                for item in cartesian {
                    expanded_words.push(Word::literal(format!("{}{}{}", prefix, item, suffix)));
                }
            }
        }
        
        expanded_words
    }
    
    /// Generate cartesian product of multiple vectors
    fn generate_cartesian_product(items: &[Vec<String>]) -> Vec<String> {
        if items.is_empty() {
            return vec![];
        }
        if items.len() == 1 {
            return items[0].clone();
        }
        
        let mut result = Vec::new();
        let first = &items[0];
        let rest = Self::generate_cartesian_product(&items[1..]);
        
        for item in first {
            for rest_item in &rest {
                result.push(format!("{}{}", item, rest_item));
            }
        }
        
        result
    }
    
    /// Convert an AST word to a MIR word with bounds
    fn from_ast_word(word: &crate::ast::Word) -> Word {
        match word {
            crate::ast::Word::Literal(s, _) => Word::literal(s.clone()),
            crate::ast::Word::Variable(var, _, _) => Word::variable(var.clone()),
            crate::ast::Word::ParameterExpansion(pe, _) => Word::parameter_expansion(pe.clone()),
            crate::ast::Word::Array(name, elements, _) => Word::array(name.clone(), elements.clone()),
            crate::ast::Word::MapAccess(map_name, key, _) => Word::map_access(map_name.clone(), key.clone()),
            crate::ast::Word::MapKeys(map_name, _) => Word::map_keys(map_name.clone()),
            crate::ast::Word::MapLength(map_name, _) => Word::map_length(map_name.clone()),
            crate::ast::Word::ArraySlice(array_name, offset, length, _) => Word::array_slice(array_name.clone(), offset.clone(), length.clone()),
            crate::ast::Word::Arithmetic(expr, _) => Word::arithmetic(expr.clone()),
            crate::ast::Word::BraceExpansion(expansion, _) => Word::brace_expansion(expansion.clone()),
            crate::ast::Word::CommandSubstitution(cmd, _) => Word::command_substitution(Box::new(*cmd.clone())),
            crate::ast::Word::StringInterpolation(interp, _) => Word::string_interpolation(interp.clone()),
        }
    }
    
    /// Convert an AST command to a MIR command with optimization information
    pub fn from_ast_command(cmd: &Command) -> MirCommand {
        match cmd {
            Command::Simple(simple_cmd) => {
                // Convert AST words to MIR words and expand brace expansions
                let mut expanded_args = Vec::new();
                for arg in &simple_cmd.args {
                    let mir_word = Self::from_ast_word(arg);
                    match &mir_word {
                        Word::BraceExpansion(expansion, _) => {
                            // Expand the brace expansion into multiple literal words
                            expanded_args.extend(Self::expand_brace_expansion(expansion));
                        }
                        _ => {
                            expanded_args.push(mir_word);
                        }
                    }
                }
                
                // Create a new SimpleCommand with expanded arguments
                let expanded_simple_cmd = SimpleCommand {
                    name: Self::from_ast_word(&simple_cmd.name),
                    args: expanded_args,
                    redirects: simple_cmd.redirects.clone(),
                    env_vars: simple_cmd.env_vars.clone(),
                    stdout_used: simple_cmd.stdout_used,
                    stderr_used: simple_cmd.stderr_used,
                };
                
                MirCommand::Simple(expanded_simple_cmd)
            },
            Command::Pipeline(pipeline) => MirCommand::Pipeline(pipeline.clone()),
            Command::Redirect(redirect) => MirCommand::Redirect(redirect.clone()),
            Command::And(left, right) => {
                MirCommand::And(
                    Box::new(MirCommand::from_ast_command(left)),
                    Box::new(MirCommand::from_ast_command(right))
                )
            },
            Command::Or(left, right) => {
                MirCommand::Or(
                    Box::new(MirCommand::from_ast_command(left)),
                    Box::new(MirCommand::from_ast_command(right))
                )
            },
            Command::For(for_loop) => {
                // Convert AST ForLoop to MIR ForLoop with optimization analysis
                let body_commands: Vec<Command> = for_loop.body.commands.clone();
                let variable_used_after = Self::is_variable_used_after_for_loop(&for_loop.variable, &body_commands);
                let variable_overwritten_before_use = Self::is_variable_overwritten_before_use(&for_loop.variable, &body_commands);
                
                // Convert AST words to MIR words and expand brace expansions in the for loop items
                let mut expanded_items = Vec::new();
                for item in &for_loop.items {
                    let mir_word = Self::from_ast_word(item);
                    match &mir_word {
                        Word::BraceExpansion(expansion, _) => {
                            // Expand the brace expansion into multiple literal words
                            expanded_items.extend(Self::expand_brace_expansion(expansion));
                        }
                        _ => {
                            expanded_items.push(mir_word);
                        }
                    }
                }
                
                MirCommand::For(MirForLoop {
                    variable: for_loop.variable.clone(),
                    items: expanded_items,
                    body: body_commands,
                    variable_used_after,
                    variable_overwritten_before_use,
                })
            },
            Command::While(while_loop) => {
                // Convert AST WhileLoop to MIR WhileLoop with optimization analysis
                let body_commands: Vec<Command> = while_loop.body.commands.clone();
                let variables_modified_in_loop = Self::get_variables_modified_in_loop(&body_commands);
                let variables_used_after_loop = Self::get_variables_used_after_loop(&body_commands);
                
                MirCommand::While(MirWhileLoop {
                    condition: while_loop.condition.clone(),
                    body: body_commands,
                    variables_modified_in_loop,
                    variables_used_after_loop,
                })
            },
            Command::If(if_stmt) => MirCommand::If(if_stmt.clone()),
            Command::Case(case_stmt) => MirCommand::Case(case_stmt.clone()),
            Command::Function(func) => MirCommand::Function(func.clone()),
            Command::Subshell(cmd) => MirCommand::Subshell(Box::new(MirCommand::from_ast_command(cmd))),
            Command::Background(cmd) => {
                MirCommand::Background(Box::new(MirCommand::from_ast_command(cmd)))
            },
            // Handle other command types by converting them to simple MIR representations
            _ => {
                // For now, we'll create a simple representation for unsupported command types
                // In a full implementation, you'd want to handle each type appropriately
                MirCommand::Simple(SimpleCommand {
                    name: Word::literal("UNSUPPORTED".to_string()),
                    args: vec![],
                    redirects: vec![],
                    env_vars: std::collections::HashMap::new(),
                    stdout_used: true,
                    stderr_used: true,
                })
            }
        }
    }
    
    /// Check if a variable is used after a for loop
    fn is_variable_used_after_for_loop(variable: &str, commands: &[Command]) -> bool {
        // This is a simplified analysis - in a real implementation, you'd need to
        // analyze the entire script to see if the variable is used after the loop
        // For now, we'll return false as a placeholder
        false
    }
    
    /// Check if a variable is overwritten before being used
    fn is_variable_overwritten_before_use(variable: &str, commands: &[Command]) -> bool {
        // This is a simplified analysis - in a real implementation, you'd need to
        // analyze the entire script to see if the variable is overwritten before use
        // For now, we'll return false as a placeholder
        false
    }
    
    /// Get variables that are modified in a loop body
    fn get_variables_modified_in_loop(commands: &[Command]) -> Vec<String> {
        // This is a simplified analysis - in a real implementation, you'd need to
        // analyze the commands to find variable assignments
        // For now, we'll return an empty vector as a placeholder
        Vec::new()
    }
    
    /// Get variables that are used after a loop
    fn get_variables_used_after_loop(commands: &[Command]) -> Vec<String> {
        // This is a simplified analysis - in a real implementation, you'd need to
        // analyze the entire script to see which variables are used after the loop
        // For now, we'll return an empty vector as a placeholder
        Vec::new()
    }
}