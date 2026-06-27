use std::collections::HashMap;

use std::ops::Deref;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum PipeOperator {
    Pipe,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ParameterExpansion {
    pub variable: String,
    pub operator: ParameterExpansionOperator,
}

impl std::fmt::Display for ParameterExpansion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.operator {
            ParameterExpansionOperator::None => write!(f, "${{{}}}", self.variable),
            ParameterExpansionOperator::UppercaseAll => write!(f, "${{{0}^^}}", self.variable),
            ParameterExpansionOperator::LowercaseAll => write!(f, "${{{0},,}}", self.variable),
            ParameterExpansionOperator::UppercaseFirst => write!(f, "${{{0}^}}", self.variable),
            ParameterExpansionOperator::RemoveLongestPrefix(pattern) => {
                write!(f, "${{{0}##{1}}}", self.variable, pattern)
            }
            ParameterExpansionOperator::RemoveShortestPrefix(pattern) => {
                write!(f, "${{{0}#{1}}}", self.variable, pattern)
            }
            ParameterExpansionOperator::RemoveLongestSuffix(pattern) => {
                write!(f, "${{{0}%%{1}}}", self.variable, pattern)
            }
            ParameterExpansionOperator::RemoveShortestSuffix(pattern) => {
                write!(f, "${{{0}%{1}}}", self.variable, pattern)
            }
            ParameterExpansionOperator::SubstituteAll(pattern, replacement) => {
                write!(f, "${{{0}//{1}/{2}}}", self.variable, pattern, replacement)
            }
            ParameterExpansionOperator::DefaultValue(default) => {
                write!(f, "${{{0}:-{1}}}", self.variable, default)
            }
            ParameterExpansionOperator::AssignDefault(default) => {
                write!(f, "${{{0}:={1}}}", self.variable, default)
            }
            ParameterExpansionOperator::ErrorIfUnset(error) => {
                write!(f, "${{{0}:?{1}}}", self.variable, error)
            }
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
    None,
    UppercaseAll,
    LowercaseAll,
    UppercaseFirst,
    RemoveLongestPrefix(String),
    RemoveShortestPrefix(String),
    RemoveLongestSuffix(String),
    RemoveShortestSuffix(String),
    SubstituteAll(String, String),
    DefaultValue(String),
    AssignDefault(String),
    ErrorIfUnset(String),
    Basename,
    Dirname,
    ArraySlice(String, Option<String>),
}

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
    MapAccess(String, String),
    MapKeys(String),
    MapLength(String),
    ArraySlice(String, String, Option<String>),
    Arithmetic(ArithmeticExpression),
    CommandSubstitution(Box<Command>),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum Word {
    Literal(String),
    Variable(String),
    ParameterExpansion(ParameterExpansion),
    Array(String, Vec<String>),
    MapAccess(String, String),
    MapKeys(String),
    MapLength(String),
    ArraySlice(String, String, Option<String>),
    Arithmetic(ArithmeticExpression),
    BraceExpansion(BraceExpansion),
    CommandSubstitution(Box<Command>),
    StringInterpolation(StringInterpolation),
}

impl std::fmt::Display for Word {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Word::Literal(s) => write!(f, "{}", s),
            Word::Variable(var) => write!(f, "${}", var),
            Word::ParameterExpansion(pe) => match &pe.operator {
                ParameterExpansionOperator::None => write!(f, "${{{}}}", pe.variable),
                ParameterExpansionOperator::UppercaseAll => write!(f, "${{{}}}", pe.variable),
                ParameterExpansionOperator::LowercaseAll => write!(f, "${{{}}}", pe.variable),
                ParameterExpansionOperator::UppercaseFirst => write!(f, "${{{}}}", pe.variable),
                ParameterExpansionOperator::RemoveLongestPrefix(pattern) => {
                    write!(f, "${{{}}}##{}", pe.variable, pattern)
                }
                ParameterExpansionOperator::RemoveShortestPrefix(pattern) => {
                    write!(f, "${{{}}}#{}", pe.variable, pattern)
                }
                ParameterExpansionOperator::RemoveLongestSuffix(pattern) => {
                    write!(f, "${{{}}}%%{}", pe.variable, pattern)
                }
                ParameterExpansionOperator::RemoveShortestSuffix(pattern) => {
                    write!(f, "${{{}}}%{}", pe.variable, pattern)
                }
                ParameterExpansionOperator::SubstituteAll(pattern, replacement) => {
                    write!(f, "${{{}}}//{}/{}", pe.variable, pattern, replacement)
                }
                ParameterExpansionOperator::DefaultValue(default) => {
                    write!(f, "${{{}}}:-{}", pe.variable, default)
                }
                ParameterExpansionOperator::AssignDefault(default) => {
                    write!(f, "${{{}}}:={}", pe.variable, default)
                }
                ParameterExpansionOperator::ErrorIfUnset(error) => {
                    write!(f, "${{{}}}:?{}", pe.variable, error)
                }
                ParameterExpansionOperator::Basename => write!(f, "${{{}}}##*/", pe.variable),
                ParameterExpansionOperator::Dirname => write!(f, "${{{}}}%/*", pe.variable),
                ParameterExpansionOperator::ArraySlice(offset, length) => {
                    if let Some(length_str) = length {
                        write!(f, "${{{}}}:{}:{}", pe.variable, offset, length_str)
                    } else {
                        write!(f, "${{{}}}:{}", pe.variable, offset)
                    }
                }
            },
            Word::Array(name, elements) => write!(f, "{}=({})", name, elements.join(" ")),
            Word::MapAccess(map_name, key) => write!(f, "{}[{}]", map_name, key),
            Word::MapKeys(map_name) => write!(f, "!{}[@]", map_name),
            Word::MapLength(map_name) => write!(f, "#{}[@]", map_name),
            Word::ArraySlice(array_name, offset, length) => {
                if let Some(length_str) = length {
                    write!(f, "${{{}}}[@]:{}:{}", array_name, offset, length_str)
                } else {
                    write!(f, "${{{}}}[@]:{}", array_name, offset)
                }
            }
            Word::Arithmetic(expr) => write!(f, "{}", expr.expression),
            Word::BraceExpansion(expansion) => {
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
            Word::CommandSubstitution(_) => write!(f, "$(...)"),
            Word::StringInterpolation(interp) => {
                let mut result = String::new();
                for part in &interp.parts {
                    match part {
                        StringPart::Literal(s) => result.push_str(s),
                        StringPart::Variable(var) => result.push_str(&format!("${}", var)),
                        StringPart::ParameterExpansion(pe) => match &pe.operator {
                            ParameterExpansionOperator::None => {
                                result.push_str(&format!("${{{}}}", pe.variable))
                            }
                            ParameterExpansionOperator::UppercaseAll => {
                                result.push_str(&format!("${{{}}}", pe.variable))
                            }
                            ParameterExpansionOperator::LowercaseAll => {
                                result.push_str(&format!("${{{}}}", pe.variable))
                            }
                            ParameterExpansionOperator::UppercaseFirst => {
                                result.push_str(&format!("${{{}}}", pe.variable))
                            }
                            ParameterExpansionOperator::RemoveLongestPrefix(pattern) => {
                                result.push_str(&format!("${{{}}}##{}", pe.variable, pattern))
                            }
                            ParameterExpansionOperator::RemoveShortestPrefix(pattern) => {
                                result.push_str(&format!("${{{}}}#{}", pe.variable, pattern))
                            }
                            ParameterExpansionOperator::RemoveLongestSuffix(pattern) => {
                                result.push_str(&format!("${{{}}}%%{}", pe.variable, pattern))
                            }
                            ParameterExpansionOperator::RemoveShortestSuffix(pattern) => {
                                result.push_str(&format!("${{{}}}%{}", pe.variable, pattern))
                            }
                            ParameterExpansionOperator::SubstituteAll(pattern, replacement) => {
                                result.push_str(&format!(
                                    "${{{}}}//{}/{}",
                                    pe.variable, pattern, replacement
                                ))
                            }
                            ParameterExpansionOperator::DefaultValue(default) => {
                                result.push_str(&format!("${{{}}}:-{}", pe.variable, default))
                            }
                            ParameterExpansionOperator::AssignDefault(default) => {
                                result.push_str(&format!("${{{}}}:={}", pe.variable, default))
                            }
                            ParameterExpansionOperator::ErrorIfUnset(error) => {
                                result.push_str(&format!("${{{}}}:?{}", pe.variable, error))
                            }
                            ParameterExpansionOperator::Basename => {
                                result.push_str(&format!("${{{}}}##*/", pe.variable))
                            }
                            ParameterExpansionOperator::Dirname => {
                                result.push_str(&format!("${{{}}}%/*", pe.variable))
                            }
                            ParameterExpansionOperator::ArraySlice(offset, length) => {
                                if let Some(length_str) = length {
                                    result.push_str(&format!(
                                        "${{{}}}:{1}:{2}",
                                        pe.variable, offset, length_str
                                    ))
                                } else {
                                    result.push_str(&format!("${{{}}}:{1}", pe.variable, offset))
                                }
                            }
                        },
                        StringPart::MapAccess(map_name, key) => {
                            result.push_str(&format!("{}[{}]", map_name, key))
                        }
                        StringPart::MapKeys(map_name) => {
                            result.push_str(&format!("!{}[@]", map_name))
                        }
                        StringPart::MapLength(map_name) => {
                            result.push_str(&format!("#{}[@]", map_name))
                        }
                        StringPart::ArraySlice(array_name, offset, length) => {
                            if let Some(length_str) = length {
                                result.push_str(&format!(
                                    "${{{}[@]}}:{}:{}",
                                    array_name, offset, length_str
                                ));
                            } else {
                                result.push_str(&format!("${{{}[@]}}:{}", array_name, offset));
                            }
                        }
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
    pub fn literal(s: String) -> Self {
        Word::Literal(s)
    }
    pub fn variable(name: String) -> Self {
        Word::Variable(name)
    }
    pub fn parameter_expansion(pe: ParameterExpansion) -> Self {
        Word::ParameterExpansion(pe)
    }
    pub fn array(name: String, elements: Vec<String>) -> Self {
        Word::Array(name, elements)
    }
    pub fn map_access(map_name: String, key: String) -> Self {
        Word::MapAccess(map_name, key)
    }
    pub fn map_keys(map_name: String) -> Self {
        Word::MapKeys(map_name)
    }
    pub fn map_length(map_name: String) -> Self {
        Word::MapLength(map_name)
    }
    pub fn array_slice(array_name: String, offset: String, length: Option<String>) -> Self {
        Word::ArraySlice(array_name, offset, length)
    }
    pub fn arithmetic(expr: ArithmeticExpression) -> Self {
        Word::Arithmetic(expr)
    }
    pub fn brace_expansion(expansion: BraceExpansion) -> Self {
        Word::BraceExpansion(expansion)
    }
    pub fn command_substitution(cmd: Box<Command>) -> Self {
        Word::CommandSubstitution(cmd)
    }
    pub fn string_interpolation(interp: StringInterpolation) -> Self {
        Word::StringInterpolation(interp)
    }

    pub fn to_string(&self) -> String {
        match self {
            Word::Literal(s) => s.to_string(),
            Word::Variable(var) => format!("${}", var),
            Word::ParameterExpansion(pe) => match &pe.operator {
                ParameterExpansionOperator::None => format!("${{{}}}", pe.variable),
                ParameterExpansionOperator::UppercaseAll => format!("${{{}}}", pe.variable),
                ParameterExpansionOperator::LowercaseAll => format!("${{{}}}", pe.variable),
                ParameterExpansionOperator::UppercaseFirst => format!("${{{}}}", pe.variable),
                ParameterExpansionOperator::RemoveLongestPrefix(pattern) => {
                    format!("${{{}}}##{}", pe.variable, pattern)
                }
                ParameterExpansionOperator::RemoveShortestPrefix(pattern) => {
                    format!("${{{}}}#{}", pe.variable, pattern)
                }
                ParameterExpansionOperator::RemoveLongestSuffix(pattern) => {
                    format!("${{{}}}%%{}", pe.variable, pattern)
                }
                ParameterExpansionOperator::RemoveShortestSuffix(pattern) => {
                    format!("${{{}}}%{}", pe.variable, pattern)
                }
                ParameterExpansionOperator::SubstituteAll(pattern, replacement) => {
                    format!("${{{}}}//{}/{}", pe.variable, pattern, replacement)
                }
                ParameterExpansionOperator::DefaultValue(default) => {
                    format!("${{{}}}:-{}", pe.variable, default)
                }
                ParameterExpansionOperator::AssignDefault(default) => {
                    format!("${{{}}}:={}", pe.variable, default)
                }
                ParameterExpansionOperator::ErrorIfUnset(error) => {
                    format!("${{{}}}:?{}", pe.variable, error)
                }
                ParameterExpansionOperator::Basename => format!("${{{}}}##*/", pe.variable),
                ParameterExpansionOperator::Dirname => format!("${{{}}}%/*", pe.variable),
                ParameterExpansionOperator::ArraySlice(offset, length) => {
                    if let Some(length_str) = length {
                        format!("${{{}}}:{1}:{2}", pe.variable, offset, length_str)
                    } else {
                        format!("${{{}}}:{1}", pe.variable, offset)
                    }
                }
            },
            Word::Array(name, elements) => format!("{}=({})", name, elements.join(" ")),
            Word::MapAccess(map_name, key) => format!("{}[{}]", map_name, key),
            Word::MapKeys(map_name) => format!("!{}[@]", map_name),
            Word::MapLength(map_name) => format!("#{}[@]", map_name),
            Word::ArraySlice(array_name, offset, length) => {
                if let Some(length_str) = length {
                    format!("${{{}}}[@]:{}:{}", array_name, offset, length_str)
                } else {
                    format!("${{{}}}[@]:{}", array_name, offset)
                }
            }
            Word::Arithmetic(expr) => expr.expression.to_string(),
            Word::BraceExpansion(expansion) => {
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
            Word::CommandSubstitution(_) => "$(...)".to_string(),
            Word::StringInterpolation(interp) => {
                let mut result = String::new();
                for part in &interp.parts {
                    match part {
                        StringPart::Literal(s) => result.push_str(s),
                        StringPart::Variable(var) => result.push_str(&format!("${}", var)),
                        StringPart::ParameterExpansion(pe) => match &pe.operator {
                            ParameterExpansionOperator::None => {
                                result.push_str(&format!("${{{}}}", pe.variable))
                            }
                            ParameterExpansionOperator::UppercaseAll => {
                                result.push_str(&format!("${{{}}}", pe.variable))
                            }
                            ParameterExpansionOperator::LowercaseAll => {
                                result.push_str(&format!("${{{}}}", pe.variable))
                            }
                            ParameterExpansionOperator::UppercaseFirst => {
                                result.push_str(&format!("${{{}}}", pe.variable))
                            }
                            ParameterExpansionOperator::RemoveLongestPrefix(pattern) => {
                                result.push_str(&format!("${{{}}}##{}", pe.variable, pattern))
                            }
                            ParameterExpansionOperator::RemoveShortestPrefix(pattern) => {
                                result.push_str(&format!("${{{}}}#{}", pe.variable, pattern))
                            }
                            ParameterExpansionOperator::RemoveLongestSuffix(pattern) => {
                                result.push_str(&format!("${{{}}}%%{}", pe.variable, pattern))
                            }
                            ParameterExpansionOperator::RemoveShortestSuffix(pattern) => {
                                result.push_str(&format!("${{{}}}%{}", pe.variable, pattern))
                            }
                            ParameterExpansionOperator::SubstituteAll(pattern, replacement) => {
                                result.push_str(&format!(
                                    "${{{}}}//{}/{}",
                                    pe.variable, pattern, replacement
                                ))
                            }
                            ParameterExpansionOperator::DefaultValue(default) => {
                                result.push_str(&format!("${{{}}}:-{}", pe.variable, default))
                            }
                            ParameterExpansionOperator::AssignDefault(default) => {
                                result.push_str(&format!("${{{}}}:={}", pe.variable, default))
                            }
                            ParameterExpansionOperator::ErrorIfUnset(error) => {
                                result.push_str(&format!("${{{}}}:?{}", pe.variable, error))
                            }
                            ParameterExpansionOperator::Basename => {
                                result.push_str(&format!("${{{}}}##*/", pe.variable))
                            }
                            ParameterExpansionOperator::Dirname => {
                                result.push_str(&format!("${{{}}}%/*", pe.variable))
                            }
                            ParameterExpansionOperator::ArraySlice(offset, length) => {
                                if let Some(length_str) = length {
                                    result.push_str(&format!(
                                        "${{{}}}:{1}:{2}",
                                        pe.variable, offset, length_str
                                    ))
                                } else {
                                    result.push_str(&format!("${{{}}}:{1}", pe.variable, offset))
                                }
                            }
                        },
                        StringPart::MapAccess(map_name, key) => {
                            result.push_str(&format!("${{{}}}[{}]", map_name, key))
                        }
                        StringPart::MapKeys(map_name) => {
                            result.push_str(&format!("${{!{}}}[@]", map_name))
                        }
                        StringPart::MapLength(map_name) => {
                            result.push_str(&format!("${{#{}}}[@]", map_name))
                        }
                        StringPart::ArraySlice(array_name, offset, length) => {
                            if let Some(length_str) = length {
                                result.push_str(&format!(
                                    "${{{}[@]}}:{}:{}",
                                    array_name, offset, length_str
                                ));
                            } else {
                                result.push_str(&format!("${{{}[@]}}:{}", array_name, offset));
                            }
                        }
                        StringPart::Arithmetic(expr) => result.push_str(&expr.expression),
                        StringPart::CommandSubstitution(_) => result.push_str("$(...)"),
                    }
                }
                format!("\"{}\"", result)
            }
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Word::Literal(s) => s,
            _ => "",
        }
    }

    pub fn as_literal(&self) -> Option<&str> {
        match self {
            Word::Literal(s) => Some(s),
            _ => None,
        }
    }
}

impl Deref for Word {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        match self {
            Word::Literal(s) => s,
            _ => "",
        }
    }
}

impl PartialEq<str> for Word {
    fn eq(&self, other: &str) -> bool {
        match self {
            Word::Literal(s) => s == other,
            Word::Variable(var) => var == other,
            Word::ParameterExpansion(pe) => pe.variable == other,
            Word::MapKeys(map_name) => map_name == other,
            Word::MapLength(map_name) => map_name == other,
            Word::Arithmetic(expr) => expr.expression == other,
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

/// Represents a span of source code with start/end positions and original text
#[derive(Debug, Clone, PartialEq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
    pub original_text: String,
}

impl SourceSpan {
    pub fn new(start: usize, end: usize, original_text: String) -> Self {
        Self {
            start,
            end,
            original_text,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum Command {
    Simple(SimpleCommand),
    BuiltinCommand(BuiltinCommand),
    ShoptCommand(ShoptCommand),
    TestExpression(TestExpression),
    Pipeline(Pipeline),
    And(Box<Command>, Box<Command>), // left && right
    Or(Box<Command>, Box<Command>),  // left || right
    If(IfStatement),
    Case(CaseStatement),
    While(WhileLoop),
    For(ForLoop),
    Function(Function),
    Subshell(Box<Command>),
    Background(Box<Command>),
    Block(Block),
    Redirect(RedirectCommand),
    Assignment(Assignment), // Variable assignment like i=value
    CStyleFor(CStyleForLoop),
    Break(Option<String>),    // Optional loop level
    Continue(Option<String>), // Optional loop level
    Return(Option<Word>),     // Optional return value
    BlankLine,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SimpleCommand {
    pub name: Word,
    pub args: Vec<Word>,
    pub redirects: Vec<Redirect>,
    pub env_vars: HashMap<String, Word>,
    pub append_vars: HashMap<String, Word>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BuiltinCommand {
    pub name: String,
    pub args: Vec<Word>,
    pub redirects: Vec<Redirect>,
    pub env_vars: HashMap<String, Word>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ShoptCommand {
    pub option: String,
    pub enable: bool, // true for -s (set), false for -u (unset)
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Pipeline {
    pub commands: Vec<Command>,
    pub operators: Vec<PipeOperator>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct IfStatement {
    pub condition: Box<Command>,
    pub then_branch: Box<Command>,
    pub else_branch: Option<Box<Command>>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CaseStatement {
    pub word: Word,
    pub cases: Vec<CaseClause>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CaseClause {
    pub patterns: Vec<Word>,
    pub body: Vec<Command>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct WhileLoop {
    pub condition: Box<Command>,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ForLoop {
    pub variable: String,
    pub items: Vec<Word>,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CStyleForLoop {
    /// The raw content inside `(( ... ))`, e.g. `"i=0; i<10; i++"`
    pub arith_content: String,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Function {
    pub name: String,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Block {
    pub commands: Vec<Command>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RedirectCommand {
    pub command: Box<Command>,
    pub redirects: Vec<Redirect>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Assignment {
    pub variable: String,
    pub value: Word,
    pub operator: AssignmentOperator,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum AssignmentOperator {
    Assign,        // =
    PlusAssign,    // +=
    MinusAssign,   // -=
    StarAssign,    // *=
    SlashAssign,   // /=
    PercentAssign, // %=
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Redirect {
    pub fd: Option<i32>,
    pub operator: RedirectOperator,
    pub target: Word,
    pub heredoc_body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum RedirectOperator {
    Input,                                   // <
    Output,                                  // >
    Append,                                  // >>
    InputOutput,                             // <>
    Heredoc,                                 // <<
    HeredocTabs,                             // <<-
    HereString,                              // <<<
    ProcessSubstitutionInput(Box<Command>),  // <(command)
    ProcessSubstitutionOutput(Box<Command>), // >(command)
    StderrOutput,                            // 2>
    StderrAppend,                            // 2>>
    StderrInput,                             // 2<
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TestExpression {
    pub expression: String,
    pub modifiers: TestModifiers,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TestModifiers {
    pub extglob: bool,
    pub nocasematch: bool,
    pub globstar: bool,
    pub nullglob: bool,
    pub failglob: bool,
    pub dotglob: bool,
}

impl Default for TestModifiers {
    fn default() -> Self {
        Self {
            extglob: false,
            nocasematch: false,
            globstar: false,
            nullglob: false,
            failglob: false,
            dotglob: false,
        }
    }
}
