# Failing Test Notes

## Remaining Combined Failures

### 063_hard_to_parse.sh
Multiple hard-to-parse constructs combined. Still has exit code and stdout
mismatches due to:
- Complex process substitution with nested subshells
- eval with complex expansions
- Brace expansion with ranges
- Backslash line continuations
- trap/shopt commands
- Background processes via fork/exec
- Complex command serialization for open3 fallback
- Nested command substitutions in test expressions

### 064_hard_to_generate.sh
Multiple hard-to-generate features combined. Stdout mismatch due to:
- Complex nested subshells with process substitution
- Brace expansion with mixed ranges and sequences
- Here-documents with variable interpolation
- Nested function definitions
- Sort ordering differences
- Simple commands (echo, ls, find, etc.) inside backticks are run via
  open3('bash', '-c', ...) instead of being translated to native Perl.
  Complex control flow (If, While, For) gets native Perl; the cheat is on
  the easy cases.
