# AST (Abstract Syntax Tree) Documentation

This document describes the structure and elements of the Abstract Syntax Tree (AST) used in the sh2perl project to represent shell scripts.

## Overview

The AST is designed to represent shell script syntax in a structured, hierarchical format that can be easily processed and transformed. It consists of two main components:

1. **Commands** - Represent executable statements and control structures
2. **Words** - Represent data elements like literals, variables, and expansions

## Core Structures

### SourceSpan

Represents a span of source code with position information:

```rust
pub struct SourceSpan {
    pub start: usize,        // Start position in source
    pub end: usize,          // End position in source
    pub original_text: String, // Original text content
}
```

## Command Types

The `Command` enum represents all possible shell commands and control structures:

### Simple Commands

**SimpleCommand** - Basic command execution:
```rust
pub struct SimpleCommand {
    pub name: Word,                    // Command name
    pub args: Vec<Word>,              // Command arguments
    pub redirects: Vec<Redirect>,     // I/O redirections
    pub env_vars: HashMap<String, Word>, // Environment variables
    pub stdout_used: bool,            // Whether stdout is used
    pub stderr_used: bool,            // Whether stderr is used
}
```

**BuiltinCommand** - Shell built-in commands:
```rust
pub struct BuiltinCommand {
    pub name: String,                 // Builtin name
    pub args: Vec<Word>,              // Command arguments
    pub redirects: Vec<Redirect>,     // I/O redirections
    pub env_vars: HashMap<String, Word>, // Environment variables
    pub stdout_used: bool,            // Whether stdout is used
    pub stderr_used: bool,            // Whether stderr is used
}
```

**ShoptCommand** - Shell option commands:
```rust
pub struct ShoptCommand {
    pub option: String,               // Option name
    pub enable: bool,                 // true for -s (set), false for -u (unset)
}
```

### Control Structures

**IfStatement** - Conditional execution:
```rust
pub struct IfStatement {
    pub condition: Box<Command>,      // Condition to test
    pub then_branch: Box<Command>,   // Commands to execute if true
    pub else_branch: Option<Box<Command>>, // Optional else branch
}
```

**CaseStatement** - Pattern matching:
```rust
pub struct CaseStatement {
    pub word: Word,                   // Value to match against
    pub cases: Vec<CaseClause>,      // List of case clauses
}

pub struct CaseClause {
    pub patterns: Vec<Word>,          // Patterns to match
    pub body: Vec<Command>,           // Commands to execute
}
```

**WhileLoop** - Conditional loops:
```rust
pub struct WhileLoop {
    pub condition: Box<Command>,      // Loop condition
    pub body: Block,                  // Loop body
}
```

**ForLoop** - Iteration loops:
```rust
pub struct ForLoop {
    pub variable: String,             // Loop variable
    pub items: Vec<Word>,             // Items to iterate over
    pub body: Block,                  // Loop body
}
```

**Function** - Function definitions:
```rust
pub struct Function {
    pub name: String,                 // Function name
    pub parameters: Vec<String>,      // Parameter names
    pub body: Block,                  // Function body
}
```

### Composite Commands

**Pipeline** - Command pipelines:
```rust
pub struct Pipeline {
    pub commands: Vec<Command>,       // Commands in the pipeline
    pub source_text: Option<String>,  // Original bash command text
    pub stdout_used: bool,            // Whether stdout is used
    pub stderr_used: bool,            // Whether stderr is used
}
```

**Block** - Command blocks:
```rust
pub struct Block {
    pub commands: Vec<Command>,       // Commands in the block
}
```

### Special Commands

**RedirectCommand** - Commands with redirections:
```rust
pub struct RedirectCommand {
    pub command: Box<Command>,        // The command to redirect
    pub redirects: Vec<Redirect>,     // Redirection specifications
}
```

**Assignment** - Variable assignments:
```rust
pub struct Assignment {
    pub variable: String,             // Variable name
    pub value: Word,                  // Value to assign
    pub operator: AssignmentOperator, // Assignment operator
}
```

### Assignment Operators

```rust
pub enum AssignmentOperator {
    Assign,      // =
    PlusAssign,  // +=
    MinusAssign, // -=
    StarAssign,  // *=
    SlashAssign, // /=
    PercentAssign, // %=
}
```

### Control Flow Commands

- `And(Box<Command>, Box<Command>)` - Logical AND (&&)
- `Or(Box<Command>, Box<Command>)` - Logical OR (||)
- `Subshell(Box<Command>)` - Subshell execution
- `Background(Box<Command>)` - Background execution
- `Break(Option<String>)` - Break statement with optional level
- `Continue(Option<String>)` - Continue statement with optional level
- `Return(Option<Word>)` - Return statement with optional value
- `BlankLine` - Empty line

## Word Types

Words represent data elements in shell scripts. The `Word` enum includes:

### Basic Words

**Literal** - String literals:
```rust
Literal(String, Option<()>)
```

**Variable** - Variable references:
```rust
Variable(String, bool, Option<()>) // name, is_mutable, annotations
```

### Expansions

**ParameterExpansion** - Parameter expansions like `${var:-default}`:
```rust
ParameterExpansion(ParameterExpansion, Option<()>)
```

**Array** - Array definitions:
```rust
Array(String, Vec<String>, Option<()>) // name, elements, annotations
```

**MapAccess** - Associative array access:
```rust
MapAccess(String, String, Option<()>) // map_name, key, annotations
```

**MapKeys** - Get associative array keys:
```rust
MapKeys(String, Option<()>) // map_name, annotations
```

**MapLength** - Get array length:
```rust
MapLength(String, Option<()>) // map_name, annotations
```

**ArraySlice** - Array slicing:
```rust
ArraySlice(String, String, Option<String>, Option<()>) // name, offset, length, annotations
```

### Complex Expansions

**Arithmetic** - Arithmetic expressions:
```rust
Arithmetic(ArithmeticExpression, Option<()>)
```

**BraceExpansion** - Brace expansions like `{a,b,c}`:
```rust
BraceExpansion(BraceExpansion, Option<()>)
```

**CommandSubstitution** - Command substitution `$(command)`:
```rust
CommandSubstitution(Box<Command>, Option<()>)
```

**StringInterpolation** - String interpolation with embedded variables:
```rust
StringInterpolation(StringInterpolation, Option<()>)
```

## Parameter Expansion

Parameter expansions support various operations:

```rust
pub enum ParameterExpansionOperator {
    None,                           // Simple variable reference
    UppercaseAll,                   // ^^ - uppercase all
    LowercaseAll,                   // ,, - lowercase all
    UppercaseFirst,                 // ^ - uppercase first
    RemoveLongestPrefix(String),    // ##pattern
    RemoveShortestPrefix(String),   // #pattern
    RemoveLongestSuffix(String),    // %%pattern
    RemoveShortestSuffix(String),   // %pattern
    SubstituteAll(String, String),  // //pattern/replacement
    DefaultValue(String),           // :-default
    AssignDefault(String),          // :=default
    ErrorIfUnset(String),          // :?error
    Basename,                       // ##*/ - get basename
    Dirname,                        // %/* - get dirname
    ArraySlice(String, Option<String>), // :offset or :start:length
}
```

## Redirections

Redirections are represented by the `Redirect` struct:

```rust
pub struct Redirect {
    pub fd: Option<i32>,            // File descriptor (None for default)
    pub operator: RedirectOperator,  // Type of redirection
    pub target: Word,                // Target file/command
    pub heredoc_body: Option<String>, // For heredoc redirections
}
```

### Redirect Operators

```rust
pub enum RedirectOperator {
    Input,                          // <
    Output,                         // >
    Append,                         // >>
    InputOutput,                    // <>
    Heredoc,                        // <<
    HeredocTabs,                    // <<-
    HereString,                     // <<<
    ProcessSubstitutionInput(Box<Command>),  // <(command)
    ProcessSubstitutionOutput(Box<Command>), // >(command)
    StderrOutput,                   // 2>
    StderrAppend,                   // 2>>
    StderrInput,                    // 2<
}
```

## Test Expressions

Test expressions for conditional statements:

```rust
pub struct TestExpression {
    pub expression: String,         // The test expression
    pub modifiers: TestModifiers,   // Shell options affecting the test
}

pub struct TestModifiers {
    pub extglob: bool,              // Extended globbing
    pub nocasematch: bool,          // Case-insensitive matching
    pub globstar: bool,             // Recursive globbing
    pub nullglob: bool,             // Empty glob results
    pub failglob: bool,             // Fail on glob errors
    pub dotglob: bool,              // Include dot files in globs
}
```

## Arithmetic Expressions

Arithmetic expressions are tokenized for processing:

```rust
pub struct ArithmeticExpression {
    pub expression: String,         // Original expression
    pub tokens: Vec<ArithmeticToken>, // Tokenized representation
}

pub enum ArithmeticToken {
    Number(String),                 // Numeric literal
    Variable(String),               // Variable reference
    Operator(String),               // Arithmetic operator
    ParenOpen,                      // Opening parenthesis
    ParenClose,                     // Closing parenthesis
}
```

## Brace Expansion

Brace expansions support various patterns:

```rust
pub struct BraceExpansion {
    pub prefix: Option<String>,     // Optional prefix
    pub items: Vec<BraceItem>,      // Expansion items
    pub suffix: Option<String>,     // Optional suffix
}

pub enum BraceItem {
    Literal(String),                // Literal string
    Range(BraceRange),              // Numeric/character range
    Sequence(Vec<String>),          // Comma-separated sequence
}

pub struct BraceRange {
    pub start: String,              // Range start
    pub end: String,                // Range end
    pub step: Option<String>,       // Optional step
    pub format: Option<String>,     // Optional format string
}
```

## String Interpolation

String interpolation supports embedded expansions:

```rust
pub struct StringInterpolation {
    pub parts: Vec<StringPart>,     // Interpolated parts
}

pub enum StringPart {
    Literal(String),                // Literal text
    Variable(String),               // Variable reference
    ParameterExpansion(ParameterExpansion), // Parameter expansion
    MapAccess(String, String),      // Map access
    MapKeys(String),                // Map keys
    MapLength(String),              // Map length
    ArraySlice(String, String, Option<String>), // Array slice
    Arithmetic(ArithmeticExpression), // Arithmetic expression
    CommandSubstitution(Box<Command>), // Command substitution
}
```

## Annotations

The AST includes `Option<()>` fields throughout for future annotations. These are reserved for MIR (Mid-level Intermediate Representation) annotations that can be added during analysis phases.

## Usage Examples

### Simple Command
```bash
echo "Hello World"
```
Becomes:
```rust
Command::Simple(SimpleCommand {
    name: Word::literal("echo".to_string()),
    args: vec![Word::literal("Hello World".to_string())],
    redirects: vec![],
    env_vars: HashMap::new(),
    stdout_used: true,
    stderr_used: false,
})
```

### Variable Assignment
```bash
name="John"
```
Becomes:
```rust
Command::Assignment(Assignment {
    variable: "name".to_string(),
    value: Word::literal("John".to_string()),
    operator: AssignmentOperator::Assign,
})
```

### If Statement
```bash
if [ -f file.txt ]; then
    echo "File exists"
fi
```
Becomes:
```rust
Command::If(IfStatement {
    condition: Box::new(Command::TestExpression(TestExpression {
        expression: "-f file.txt".to_string(),
        modifiers: TestModifiers::default(),
    })),
    then_branch: Box::new(Command::Simple(SimpleCommand {
        name: Word::literal("echo".to_string()),
        args: vec![Word::literal("File exists".to_string())],
        redirects: vec![],
        env_vars: HashMap::new(),
        stdout_used: true,
        stderr_used: false,
    })),
    else_branch: None,
})
```

This AST structure provides a comprehensive representation of shell script syntax that can be easily processed, analyzed, and transformed into other formats like Perl code.
