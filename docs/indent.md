# Indentation System Documentation

## Overview

The sh2perl indentation system is designed to generate properly formatted Perl code that maintains the logical structure and readability of the original shell script. The system uses a global indentation level tracked in the `Generator` struct and applies consistent 4-space indentation throughout the generated code.

## Core Components

### Generator Struct
```rust
pub struct Generator {
    pub indent_level: usize,  // Current indentation level (0-based)
    // ... other fields
}
```

### Indentation Method
```rust
pub fn indent(&self) -> String {
    "    ".repeat(self.indent_level)  // 4 spaces per level
}
```

## Indentation Rules

### 1. Base Indentation Unit
- **4 spaces** per indentation level
- No tabs are used
- Consistent across all generated code

### 2. Indentation Level Management
- **Increment** (`indent_level += 1`) when entering:
  - Block statements (`{ ... }`)
  - Control flow structures (`if`, `while`, `for`, `else`)
  - Function bodies
  - Pipeline command bodies
  - Subshell contexts

- **Decrement** (`indent_level -= 1`) when exiting:
  - Block statements
  - Control flow structures
  - Function bodies
  - Pipeline command bodies

### 3. Indentation Application Patterns

#### Pattern 1: Direct Indentation
```rust
output.push_str(&generator.indent());
output.push_str("my $variable = value;\n");
```
**Result:**
```perl
    my $variable = value;
```

#### Pattern 2: Block Entry/Exit
```rust
output.push_str("while (condition) {\n");
generator.indent_level += 1;
output.push_str(&generator.indent());
output.push_str("body_statement;\n");
generator.indent_level -= 1;
output.push_str(&generator.indent());
output.push_str("}\n");
```
**Result:**
```perl
while (condition) {
    body_statement;
}
```

#### Pattern 3: Multi-line Command Output
```rust
let command_output = generate_linebyline_command(generator, cmd, "line", index);
for line in command_output.lines() {
    output.push_str(&generator.indent());
    output.push_str(line);
    output.push_str("\n");
}
```
**Result:** Each line of the command output gets the current indentation level applied.

## Special Cases

### 1. Pipeline Commands
Pipeline commands often generate multi-line output that needs consistent indentation. The system handles this by:
- Applying base indentation to each line of command output
- Commands can generate their own relative indentation within their output
- The caller is responsible for applying the base indentation level

### 2. Nested Structures
When commands generate nested structures (like `if/else` blocks), they should:
- Generate their own relative indentation for internal structure
- Rely on the caller to provide the base indentation level
- Use consistent 4-space increments for internal nesting

### 3. Variable Declarations
Variable declarations follow special rules:
- Only declared at `indent_level <= 1` to avoid redeclaring in nested scopes
- This prevents variable shadowing issues in loops and nested blocks

## Key Insights and Lessons Learned

### The Double Indentation Problem
**Discovery:** Commands that generate internal structures (like `if/else` blocks) can suffer from "double indentation" when both the command and the caller apply indentation.

**Problem Pattern:**
```rust
// Command generates internal structure with indentation
output.push_str("if (condition) {\n");
output.push_str("    statement;\n");  // 4 spaces internal
output.push_str("}\n");

// Caller applies base indentation to each line
for line in command_output.lines() {
    output.push_str(&generator.indent());  // 4 spaces base
    output.push_str(line);                 // + 4 spaces internal = 8 total
    output.push_str("\n");
}
```

**Result:** Incorrect double indentation
```perl
    if (condition) {
        statement;  // 8 spaces instead of 4
    }
```

### The Solution: Separation of Concerns
**Key Insight:** Commands should generate **unindented** internal structures and let the caller handle all indentation.

**Correct Pattern:**
```rust
// Command generates unindented internal structure
output.push_str("if (condition) {\n");
output.push_str("    statement;\n");  // 4 spaces internal only
output.push_str("}\n");

// Caller applies base indentation to each line
for line in command_output.lines() {
    output.push_str(&generator.indent());  // 4 spaces base
    output.push_str(line);                 // + 4 spaces internal = 8 total
    output.push_str("\n");
}
```

**Result:** Correct indentation
```perl
    if (condition) {
        statement;  // 8 spaces total (4 base + 4 internal)
    }
```

### The Indentation Hierarchy Principle
**Rule:** Commands should generate their internal structure with **relative indentation** (relative to their own base), not absolute indentation.

- **Base Level (0)**: Top-level statements
- **Level 1 (4 spaces)**: Inside blocks, loops, conditions
- **Level 2 (8 spaces)**: Inside nested structures within commands
- **Level 3 (12 spaces)**: Deeply nested structures

## Current Issues and Solutions

### Issue: Inconsistent Indentation in Generated Commands
**Problem:** Some commands (like `head`) generate internal structures that don't properly align with the base indentation level.

**Current State:**
```perl
    if ($head_count_0 < 3) {
    $head_count_0++;
} else {
    last;
}
```

**Expected State:**
```perl
    if ($head_count_0 < 3) {
        $head_count_0++;
    } else {
        last;
    }
```

**Solution:** Commands that generate internal structures should:
1. Generate their own relative indentation for internal elements
2. Use consistent 4-space increments
3. Ensure proper alignment of braces and statements

### Issue: Multi-line Command Output Indentation
**Problem:** When commands generate multi-line output, only the first line gets proper indentation.

**Solution:** The caller should iterate over all lines and apply indentation to each:
```rust
for line in command_output.lines() {
    output.push_str(&generator.indent());
    output.push_str(line);
    output.push_str("\n");
}
```

## Real-World Example: The Head Command Fix

### The Problem
The `head` command generates an `if/else` block to limit output lines. Initially, it had incorrect indentation:

```perl
    if ($head_count_0 < 3) {
    $head_count_0++;  // Missing indentation
} else {
    last;             // Missing indentation
}
```

### The Root Cause
The head command was generating internal structure without proper relative indentation, and the caller was applying base indentation to each line, resulting in misaligned code.

### The Solution
1. **Command Level**: Generate internal structure with proper relative indentation
2. **Caller Level**: Apply base indentation to all lines

**Implementation:**
```rust
// In head command generation
output.push_str(&format!("if ($head_count_0 < {}) {{\n", cmd_index, num_lines));
output.push_str(&format!("    $head_count_0++;\n", cmd_index));  // 4 spaces internal
output.push_str("} else {\n");
output.push_str("    last;\n");                                   // 4 spaces internal
output.push_str("}\n");

// In caller (yes command processing)
for line in command_output.lines() {
    output.push_str(&generator.indent());  // 4 spaces base
    output.push_str(line);                 // + 4 spaces internal = 8 total
    output.push_str("\n");
}
```

**Result:**
```perl
    if ($head_count_0 < 3) {
        $head_count_0++;  // 8 spaces total (4 base + 4 internal)
    } else {
        last;             // 8 spaces total (4 base + 4 internal)
    }
```

## Best Practices

### 1. Command Generation
- Commands should generate their own internal structure with proper relative indentation
- Don't rely on the caller to handle internal indentation
- Use consistent 4-space increments for nested structures

### 2. Caller Responsibilities
- Apply base indentation level to all lines of command output
- Don't assume commands handle their own base indentation
- Maintain proper indentation level tracking

### 3. Block Structure
- Always increment `indent_level` before generating block content
- Always decrement `indent_level` after generating block content
- Ensure proper pairing of increment/decrement operations

### 4. Error Prevention
- Reset `indent_level = 0` for top-level commands to prevent staircase effect
- Use consistent patterns for block entry/exit
- Test generated code for proper indentation

## Debugging Indentation Issues

### Common Problems and How to Identify Them

#### 1. Double Indentation
**Symptoms:** Statements appear over-indented (8+ spaces when they should be 4)
**Cause:** Both command and caller are applying indentation
**Fix:** Make command generate unindented output, let caller handle all indentation

#### 2. Missing Internal Indentation
**Symptoms:** Statements inside blocks appear at the same level as the block opening
**Cause:** Command not generating internal relative indentation
**Fix:** Add proper internal indentation within command generation

#### 3. Inconsistent Brace Alignment
**Symptoms:** Opening and closing braces don't align properly
**Cause:** Mixed indentation approaches or incorrect level management
**Fix:** Ensure consistent indentation level management

#### 4. Staircase Effect
**Symptoms:** Indentation level keeps increasing without proper reset
**Cause:** Missing `indent_level -= 1` or incorrect level management
**Fix:** Ensure proper pairing of increment/decrement operations

### Debugging Tools and Techniques

#### 1. Visual Inspection
- Look for misaligned braces and statements
- Check for consistent 4-space increments
- Verify proper nesting hierarchy

#### 2. Code Analysis
- Trace indentation level changes in the code
- Verify proper increment/decrement pairing
- Check command output generation patterns

#### 3. Test Cases
- Use simple shell scripts to test indentation
- Test with nested structures (loops, conditions)
- Verify multi-line command output handling

## Testing and Validation

### Visual Inspection
- Generated Perl code should be properly indented and readable
- Control structures should have consistent brace alignment
- Nested blocks should have clear visual hierarchy

### Automated Testing
- Test with various shell script structures
- Verify indentation consistency across different command types
- Ensure no indentation drift or staircase effects

## Future Improvements

### 1. Indentation Validation
- Add runtime checks to ensure proper indentation level management
- Detect mismatched increment/decrement operations
- Validate generated code indentation

### 2. Configurable Indentation
- Allow different indentation styles (2 spaces, tabs)
- Support different brace styles
- Make indentation configurable per project

### 3. Better Error Messages
- Provide clear error messages for indentation issues
- Help developers understand indentation problems
- Suggest fixes for common indentation errors

## Examples

### Simple Variable Declaration
```rust
output.push_str(&generator.indent());
output.push_str("my $variable = 'value';\n");
```
**Output:**
```perl
    my $variable = 'value';
```

### While Loop with Body
```rust
output.push_str("while (condition) {\n");
generator.indent_level += 1;
output.push_str(&generator.indent());
output.push_str("body_statement;\n");
generator.indent_level -= 1;
output.push_str(&generator.indent());
output.push_str("}\n");
```
**Output:**
```perl
while (condition) {
    body_statement;
}
```

### Nested If/Else Structure
```rust
output.push_str("if (condition) {\n");
generator.indent_level += 1;
output.push_str(&generator.indent());
output.push_str("true_branch;\n");
generator.indent_level -= 1;
output.push_str(&generator.indent());
output.push_str("} else {\n");
generator.indent_level += 1;
output.push_str(&generator.indent());
output.push_str("false_branch;\n");
generator.indent_level -= 1;
output.push_str(&generator.indent());
output.push_str("}\n");
```
**Output:**
```perl
if (condition) {
    true_branch;
} else {
    false_branch;
}
```

This documentation provides a comprehensive guide to understanding and working with the sh2perl indentation system.
