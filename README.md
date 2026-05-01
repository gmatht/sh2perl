# Debashc - Shell Script Converter

[![Tests](https://github.com/gmatht/sh2perl/actions/workflows/test.yml/badge.svg)](https://github.com/gmatht/sh2perl/actions/workflows/test.yml)

<!-- Dynamic badges for test counts served from gh-pages branch -->
[![Purify tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/gmatht/sh2perl/gh-pages/.github/badges/purify.json)](https://github.com/gmatht/sh2perl/actions/workflows/test.yml)
[![Main tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/gmatht/sh2perl/gh-pages/.github/badges/main-tests.json)](https://github.com/gmatht/sh2perl/actions/workflows/test.yml)

A comprehensive Rust library and command-line tool for parsing and converting shell/bash scripts to other programming languages. This project provides a robust foundation for analyzing shell scripts, converting them to Perl, Rust, Python, and more, or building shell script analysis tools.

**🌐 [Try the Live Demo](https://dansted.org/Debashc8/) - Convert shell scripts to Perl, Rust, and other languages in your browser!**

## Features

- **Complete Lexer**: Tokenizes shell/bash scripts with support for all major shell constructs
- **AST Parser**: Converts tokens into a structured Abstract Syntax Tree
- **Multi-language Code Generation**: Convert shell scripts to Perl, Rust, and Python
- **Web Assembly Support**: Run in the browser with a beautiful web interface
- **Shell Constructs Support**:
  - Simple commands and arguments
  - Pipelines and redirections
  - Control flow (if/else, loops, functions)
  - Variable expansions
  - Command substitutions
  - Arithmetic expressions
  - File test operators
  - Comments and whitespace

## Why Not Use an LLM Instead?

While Large Language Models (LLMs) like GPT-4 or Claude can translate shell scripts to other languages, this specialized transcoder offers several key advantages:

### **Reliability & Consistency**
- **Deterministic Output**: Every conversion produces identical, predictable results
- **No Hallucinations**: Unlike LLMs, this tool won't invent non-existent functions or syntax
- **Consistent Style**: Maintains uniform code formatting and structure across all conversions

### **Performance & Cost**
- **Lightning Fast**: Converts scripts in milliseconds without API calls or network latency
- **Zero Cost**: No API usage fees, token costs, or rate limits
- **Offline Operation**: Works completely offline without internet connectivity

### **Accuracy & Understanding**
- **Deep Shell Knowledge**: Built specifically for shell script semantics, not general text understanding
- **Proper AST Parsing**: Creates accurate Abstract Syntax Trees that preserve script logic
- **Language-Specific Optimizations**: Generates idiomatic code for target language

### **Integration & Automation**
- **CI/CD Ready**: Can be integrated into build pipelines and automated workflows
- **Library API**: Provides programmatic access for embedding in other tools
- **Batch Processing**: Efficiently handles multiple files without API constraints

### **When This Transcoder Excels**
This tool is ideal for:
- Automated conversion pipelines
- Performance-critical applications
- Offline or air-gapped systems
- Consistent, repeatable conversions

## Installation

```bash
git clone https://github.com/gmatht/debashc.git
cd debashc
cargo build --release
```

## Usage

### Command Line Interface

The `debashc` binary provides several commands for analyzing and converting shell scripts:

```bash
# Tokenize a shell script
debashc lex "echo hello world"

# Parse a shell script to AST
debashc parse "ls | grep test"

# Parse a shell script from file
debashc file examples/simple.sh

# Convert shell script to Perl
debashc parse --perl "ls | grep test"

# Convert shell script to Rust
debashc parse --rust "ls | grep test"

# Convert shell script to Python
debashc parse --python "ls | grep test"

# Convert shell script file to Perl
debashc file --perl examples/simple.sh

# Interactive mode
debashc interactive
```

### Library Usage

```rust
use debashc::{Lexer, Parser, Generator, RustGenerator, PythonGenerator};

// Tokenize a shell script
let input = "echo hello world";
let mut lexer = Lexer::new(input);
while let Some(token) = lexer.next() {
    println!("Token: {:?}", token);
}

// Parse a shell script
let mut parser = Parser::new(input);
match parser.parse() {
    Ok(commands) => {
        for command in commands {
            println!("Command: {:?}", command);
        }
    }
    Err(e) => eprintln!("Parse error: {}", e),
}

// Convert to Perl
let mut generator = Generator::new();
let perl_code = generator.generate(&commands);
println!("Perl code: {}", perl_code);
```

## Web Interface

Debashc includes a beautiful web interface powered by WebAssembly. You can run shell script conversions directly in your browser!

**🌐 [Live Demo](https://dansted.org/Debashc4/) - Try it now!**

### Building and Running the Web Demo

```bash
# Build the WASM target
./build-wasm.sh

# Or manually:
cargo install wasm-pack
wasm-pack build --target web --out-dir www/pkg

# Serve the web interface
cd www
python3 -m http.server 8000
# Then open http://localhost:8000 in your browser
```

The web interface provides:
- Real-time shell script tokenization
- AST visualization
- Conversion to Perl, Rust, and Python
- Interactive examples
- Responsive design for mobile and desktop

## Examples

### Simple Command
```bash
echo "Hello, World!"
```
Parses to a `SimpleCommand` with name "echo" and arguments.

### Pipeline
```bash
ls | grep "\.txt$" | wc -l
```
Parses to a `Pipeline` with multiple commands connected by pipe operators.

### Control Flow
```bash
if [ -f file.txt ]; then
    echo "File exists"
else
    echo "File does not exist"
fi
```
Parses to an `IfStatement` with condition, then branch, and else branch.

### Function Definition
```bash
function greet() {
    echo "Hello, $1!"
}
```
Parses to a `Function` with name and body.

## Shell to Perl Conversion

The `--perl` flag allows you to convert shell scripts to equivalent Perl code:

### Basic Command Conversion
```bash
# Shell script
echo "Hello, World!"

# Convert to Perl
sh2perl parse --perl 'echo "Hello, World!"'
```

Output:
```perl
print "Hello, World!\n";
```

### Pipeline Conversion
```bash
# Shell script
ls | grep "\.txt$" | wc -l

# Convert to Perl
sh2perl parse --perl 'ls | grep "\.txt$" | wc -l'
```

Output:
```perl
my $output;
$output = `ls`;
$output = `echo "$output" | grep "\.txt$"`;
print $output;
```

### Control Flow Conversion
```bash
# Shell script
if [ -f file.txt ]; then
    echo "File exists"
else
    echo "File does not exist"
fi

# Convert to Perl
sh2perl parse --perl 'if [ -f file.txt ]; then echo "File exists"; else echo "File does not exist"; fi'
```

Output:
```perl
if (-f 'file.txt') {
    print "File exists\n";
} else {
    print "File does not exist\n";
}
```

### File Operations
```bash
# Shell script
mkdir newdir
cp file.txt newdir/
rm oldfile.txt

# Convert to Perl
sh2perl parse --perl 'mkdir newdir; cp file.txt newdir/; rm oldfile.txt'
```

Output:
```perl
mkdir('newdir') or die "Cannot create directory: $!\n";
use File::Copy;
copy('file.txt', 'newdir/') or die "Cannot copy file: $!\n";
unlink('oldfile.txt') or die "Cannot remove file: $!\n";
```

### Converting Shell Script Files

You can also convert entire shell script files to Perl:

```bash
# Create a shell script file
cat > example.sh << 'EOF'
#!/bin/bash
if [ -d /tmp ]; then
    echo "Directory exists"
    ls /tmp | head -5
else
    echo "Directory does not exist"
fi
EOF

# Convert the file to Perl
sh2perl file --perl example.sh
```

Output:
```perl
if (-d '/tmp') {
    print "Directory exists\n";
    opendir(my $dh, '/tmp') or die "Cannot open directory: $!\n";
    while (my $file = readdir($dh)) {
        print "$file\n" unless $file =~ /^\.\.?$/;
    }
    closedir($dh);
} else {
    print "Directory does not exist\n";
}
```

## Project Structure

```
sh2perl/
├── src/
│   ├── lib.rs          # Library entry point
│   ├── main.rs         # Binary entry point
│   ├── lexer.rs        # Tokenizer implementation
│   ├── parser.rs       # Parser implementation
│   └── ast.rs          # Abstract Syntax Tree definitions
├── examples/           # Example shell scripts
├── tests/              # Test files
└── Cargo.toml          # Project configuration
```

## API Documentation

### Lexer

The `Lexer` struct provides tokenization of shell scripts:

```rust
pub struct Lexer {
    tokens: Vec<(Token, usize, usize)>,
    current: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self;
    pub fn next(&mut self) -> Option<&Token>;
    pub fn peek(&self) -> Option<&Token>;
    pub fn is_eof(&self) -> bool;
}
```

### Parser

The `Parser` struct converts tokens into an AST:

```rust
pub struct Parser {
    lexer: Lexer,
}

impl Parser {
    pub fn new(input: &str) -> Self;
    pub fn parse(&mut self) -> Result<Vec<Command>, ParserError>;
}
```

### AST Nodes

The AST is composed of various command types:

- `SimpleCommand`: Basic commands with arguments and redirections
- `Pipeline`: Multiple commands connected by operators
- `IfStatement`: Conditional execution
- `WhileLoop`: Loop constructs
- `ForLoop`: Iteration over lists
- `Function`: Function definitions
- `Subshell`: Commands in subshells

## Testing

Run the test suite:

```bash
cargo test
```

Run specific test categories:

```bash
cargo test lexer
cargo test parser
```

## Creating Good Examples

When contributing examples to test the shell-to-Perl converter, follow these guidelines to create effective test cases that both validate important features and make debugging easier:

### 1. **Test Specific Features**
- Focus on one or two shell constructs per example
- Cover edge cases and complex scenarios
- Test both basic and advanced usage patterns
- Include examples that stress-test the parser and generator

### 2. **Use Clear, Descriptive Names**
- Use descriptive filenames: `001_simple.sh`, `002_control_flow.sh`, `030_arrays_associative.sh`
- Include comments explaining what the example demonstrates
- Group related examples with consistent naming patterns

### 3. **Add Validation Comments**
Use special comment directives to validate the generated Perl code:

```bash
#!/bin/bash

# Test basic echo functionality
echo "Hello, World!"

# Ensure the generated Perl uses print with newline
#PERL_MUST_CONTAIN: print "Hello, World!\n"

# Ensure it doesn't use unnecessary backticks
#PERL_MUST_NOT_CONTAIN: `echo
```

**Available directives:**
- `#PERL_MUST_CONTAIN: pattern` - Generated Perl must contain this pattern
- `#PERL_MUST_NOT_CONTAIN: pattern` - Generated Perl must NOT contain this pattern
- `#AST_MUST_CONTAIN: pattern` - AST representation must contain this pattern
- `#AST_MUST_NOT_CONTAIN: pattern` - AST representation must NOT contain this pattern

### 4. **Make Examples Self-Contained**
- Include all necessary setup and cleanup
- Use temporary files with predictable names
- Clean up after the test completes
- Avoid dependencies on external files or system state

```bash
#!/bin/bash

# Create test files
echo "test content" > test_file.txt
cp test_file.txt test_file_copy.txt

# Test file operations
if [ -f test_file.txt ]; then
    echo "File exists"
fi

# Cleanup
rm -f test_file.txt test_file_copy.txt
```

### 5. **Test Both Success and Failure Cases**
- Include examples that should work correctly
- Test error handling and edge cases
- Verify that both shell and Perl versions produce identical output

### 6. **Use Meaningful Output**
- Include `echo` statements to verify correct execution
- Use descriptive output that makes it easy to spot differences
- Avoid silent operations that don't produce observable results

```bash
#!/bin/bash

echo "=== Testing file operations ==="
if [ -f "nonexistent.txt" ]; then
    echo "File exists (unexpected)"
else
    echo "File does not exist (expected)"
fi
```

### 7. **Test Complex Constructs Gradually**
- Start with simple examples and build complexity
- Test nested constructs (loops in functions, conditionals in loops)
- Include examples with multiple shell features combined

### 8. **Document Expected Behavior**
- Add comments explaining what the script should do
- Note any special considerations or edge cases
- Include expected output in comments when helpful

### 9. **Run Tests Locally**
Before submitting, test your examples:

```bash
# Test a specific example
cargo run -- test examples/your_example.sh

# Run all tests
cargo run -- test-all

# Test with Perl::Critic enabled
cargo run -- test-all --perl-critic
```

### 10. **Example Structure Template**

```bash
#!/bin/bash

# Brief description of what this example tests
# This example demonstrates [specific shell feature]

echo "=== [Test Category] ==="

# Setup (if needed)
# Create test files, set variables, etc.

# Test the main functionality
# [Shell commands being tested]

# Validation comments
#PERL_MUST_CONTAIN: expected_pattern
#PERL_MUST_NOT_CONTAIN: forbidden_pattern

# Cleanup (if needed)
# Remove temporary files, reset state
```

### 11. **Common Patterns to Test**
- **File operations**: `cp`, `mv`, `rm`, `mkdir`, `touch`
- **Text processing**: `grep`, `sed`, `awk`, `sort`, `uniq`
- **Control flow**: `if/else`, `for` loops, `while` loops, `case` statements
- **Functions**: Definition, calling, parameter passing, return values
- **Variables**: Assignment, expansion, parameter expansion
- **Pipelines**: Simple and complex command chains
- **Redirections**: Input/output redirection, here documents
- **Arrays**: Indexed and associative arrays
- **Arithmetic**: Basic and complex expressions
- **Error handling**: Exit codes, error conditions

Following these guidelines will help create robust test cases that effectively validate the converter's functionality and make it easier to identify and fix issues.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for new functionality
5. Run the test suite
6. Submit a pull request

## License

This project is licensed under the GPLv3 License - see the LICENSE file for details.

## Roadmap

- [ ] Support for more shell features (case statements etc.)
- [ ] Support for more builtins (build in awk, sed, sleep etc.)
- [ ] Shell script to other language converters
- [ ] Test more examples

## NOTES
- The binary is currently debashc. If you see references to sh2perl, well in the future I might have seperate sh2perl, sh2rust etc. Alas not yet../f

- We are trying to generate the same output as `LANG=C bash` on Linux/WSL would generate

## WASM Optimization

The project includes several optimizations to reduce WASM file size:
- Release mode builds with size optimization (`opt-level = "z"`)
- Link-time optimization (LTO)
- wasm-opt post-processing with `-O4` optimization level
- Panic abort to remove unwinding code
- Symbol stripping

Expected size reduction: 60-80% (from ~776KB to ~150-300KB)
