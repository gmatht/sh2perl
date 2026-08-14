## Creating Good Examples

When contributing examples to test the shell-to-Perl converter, follow these guidelines to create effective test cases that both validate important features and make debugging easier:

### 1. **Test Specific Features**
- Focus on one or two shell constructs per example
- Cover edge cases and complex scenarios
- Test both basic and advanced usage patterns
- Include examples that stress-test the parser and generator
- Output diagnistic info to stdout so stdout will not match original if translation is incorrect.

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
cargo run --bin debashc -- test examples/your_example.sh

# Run all tests
cargo run --bin debashc -- test-all

# Test with Perl::Critic enabled
cargo run --bin debashc -- test-all --perl-critic
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


