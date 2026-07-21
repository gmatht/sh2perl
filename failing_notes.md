# Failing Test Notes

## Tests fixed in this session (no remaining changes needed)

- 064_22_function_returning_complex_data_structures.sh — passing
- 000__04b_file_directory_operations.sh — passing
- 000__04h_complex_examples.sh — passing
- 000__07_find_path_commands.sh — passing
- 058_advanced_bash_idioms.sh — passing

## Still failing tests (4)

### 063_15_complex_function_definition.sh
`eval "$name() { $body }"` — both `$name` and `$body` are runtime function
arguments, so the translator cannot generate a static Perl `sub` at compile
time. The `system('bash', '-c', "eval ...")` fallback defines the function
in a subprocess that immediately exits, so subsequent calls to the
dynamically-defined function fail. A proper fix would require intercepting
the eval pattern and generating a real Perl subroutine that wraps the bash
body, plus changing the command dispatcher to call it instead of `system()`.

### 063_hard_to_parse.sh
Multiple issues:
1. Undeclared variables `$var` and `$file` in test expressions — used before
   any assignment, so no `my $var;` is emitted before the first use.
2. Command substitution `"$(echo "$var" | grep -q "pattern")"` inside a test
   expression — the test-expression generator passes raw bash `$(...)` syntax
   through, which is not valid Perl.
3. Complex nested subshells, process substitutions, and edge cases.
Fixing these requires either hoisting all variable declarations to the top
or handling `$(...)` inside test expressions.

### 064_07_complex_array_operations.sh
Perl hash randomization causes `values %config` to return values in a
non-deterministic order, while bash's `"${config[@]}"` gives a specific
(deterministic) hash-bucket order. Replicating bash's hash function
(`h = h * 127 + c` with per-table-size bucket selection) is impractical
because the table size depends on the number of entries and is not exposed.
A fix would require either maintaining the original key-insertion order
(by changing `env_vars` from `BTreeMap` to an ordered structure) and
iterating in that order, or using a tied hash with a fixed traversal order.

### 064_hard_to_generate.sh
Complex script combining many hard-to-generate features: process substitution
with nested pipelines, command substitutions in arithmetic, here-documents
with variable interpolation, extended glob patterns, nested function
definitions, and array slicing. Multiple undeclared variable errors and
scoping issues in the generated Perl code. The subshell variable-localization
code (`my $var = $var if defined $var;`) fails for variables that are arrays
or hashes, causing "Global symbol requires explicit package name" errors.
