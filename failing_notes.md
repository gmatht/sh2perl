# Failing Test Notes

## Tests fixed in this session (no remaining changes needed)

- 064_22_function_returning_complex_data_structures.sh — passing
- 000__04b_file_directory_operations.sh — passing
- 000__04h_complex_examples.sh — passing
- 000__07_find_path_commands.sh — passing
- 058_advanced_bash_idioms.sh — passing
- 063_15_complex_function_definition.sh — passing (fixed: added Word::Arithmetic handling in local declaration parser and generator)
- 064_hard_to_generate.sh — passing (fixed: subshell variable localization no longer references undeclared variables; added @/% declarations for type mismatches)

## Still failing tests (2)

### 063_hard_to_parse.sh
Multiple issues remaining:
1. The `while IFS= read -r line && [ -n "$line" ] && (( counter < max_lines ))` loop
   condition generates invalid Perl (multiple statements inside `while (...)`).
   Fixing requires rewriting how block conditions in while loops are translated.
2. Complex nested while loop with process substitution (`done < <(grep ...)`)
   generates malformed Perl code.

### 064_07_complex_array_operations.sh  
The test uses `IFS=$'\n' sorted=($(sort <<<"${config[*]}"))` to sort config values,
but the parser/generator does not handle process substitution with here-strings
inside array assignments. The generated Perl is completely wrong (`@sorted =
('\$(sort', '<<<\${config[*]}))', 'echo', ...)`). Fixing requires handling the
`<<<` here-string token inside process substitution in array assignment context,
or properly parsing `$(sort <<<...)` as a command substitution within the array
assignment parentheses.
