# Failing Test Notes

## Tests fixed in this session (no remaining changes needed)

- 064_22_function_returning_complex_data_structures.sh — passing
- 000__04b_file_directory_operations.sh — passing
- 000__04h_complex_examples.sh — passing
- 000__07_find_path_commands.sh — passing
- 058_advanced_bash_idioms.sh — passing
- 063_15_complex_function_definition.sh — passing (fixed: added Word::Arithmetic handling in local declaration parser and generator)

## Still failing tests (3)

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
The test uses `IFS=$'\n' sorted=($(sort <<<"${config[*]}"))` to sort config values,
but the parser/generator does not handle process substitution with here-strings
inside array assignments. The generated Perl is completely invalid (`@sorted =
('\$(sort', '<<<\${config[*]}))', 'echo', ...)`). Fixing requires handling the
`<<<` here-string token inside process substitution in array assignment context.

### 064_hard_to_generate.sh
Complex script combining many hard-to-generate features. The generated Perl
has multiple undeclared variable errors (`$config`, `$numbers`, `$output_*`)
and the subshell variable-localization code fails for array/hash variables.
