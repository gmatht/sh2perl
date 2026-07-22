# Failing Test Notes

## Known Difficult Features (Isolated Test Cases)

These 13 test cases each exercise exactly one difficult feature,
making it easy to identify which translator component needs fixing.

### 071 — `while IFS= read -r line` with env-prefix
Parser wraps env-prefixed `while` conditions in a `Block`, generating invalid code.

### 072 — Background `fork()`/`exec()` via `&` and `wait`
Subprocess management not correctly translated.

### 073 — `trap` signals
`trap` builtin has no Perl equivalent. Translator emits a comment instead.

### 074 — `shopt`
`shopt` builtin has no Perl equivalent.

### 075 — `eval` with complex expansions
Dynamic code evaluation with variable substitution inside the eval string.

### 076 — Brace expansion `{1..5}` mixed with literals and ranges
Cartesian product generation for brace expansion with mixed groups.

### 077 — Backslash line continuations in pipelines
Parser skips continuation lines, treating them as separate commands.

### 078 — `(( i = 1 + (2 * 3) / 4 ))` double-paren arithmetic
The `((...))` parser implementation may not handle nested expressions or
post-increment operators correctly.

### 079 — Here-doc with variable interpolation
Here-document body with `$variable` produces literal text instead of
interpolated output.

### 080 — `paste file1 file2` pipeline (process substitution placeholder)
Process substitution in simple pipelines (not using `<(...)` syntax).

### 081 — Nested function definitions
Functions defined inside other functions produce "will not stay shared"
warnings and may not capture enclosing variables correctly.

### 082 — Sort locale/collation
Perl's `sort` may order differently than the system `sort` command.

### 083 — Process substitution with missing files
Error handling when files referenced in process substitution don't exist.

## Remaining Combined Failures

### 063_hard_to_parse.sh
Multiple hard-to-parse constructs combined. No longer times out (timeout fixed by
proper `read` variable assignment and EOF handling in while-loop conditions),
but still has exit code and stdout mismatches due to:
- Complex process substitution with nested subshells
- eval with complex expansions
- Brace expansion with ranges
- Backslash line continuations
- trap/shopt commands
- Background processes via fork/exec

### 064_hard_to_generate.sh
Multiple hard-to-generate features combined. Stdout mismatch due to:
- Complex nested subshells with process substitution
- Brace expansion with mixed ranges and sequences
- Here-documents with variable interpolation
- Nested function definitions
- Sort ordering differences
