# Failing Test Notes

## Fixed since last update
- 086_if_condition_pipe.sh: Fixed test expression parser double-advance bugs
  and missing spaces in file test operators
- 088_while_read_ifs_sort.sh: Fixed echo output capture in pipeline context
- 091_while_pipe_var.sh: Fixed echo output capture in pipeline context
- 094_until_loop.sh: Added parse_until_loop function and generation support
- 010_pattern_matching.sh: Fixed extglob regex generation (wrong anchor)
- 037_pattern_matching_extglob.sh: Fixed extglob regex generation (wrong anchor)

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
- Simple commands inside backticks use open3('bash', '-c', ...) instead of
  native Perl (complex control flow like If/While/For is correctly translated)
