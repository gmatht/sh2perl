# Failing Test Notes

## Tests fixed in this session (no remaining changes needed)

- 049_local.sh — passing
- 051_primes.sh — passing
- 055_factorize.sh — passing
- 058_advanced_bash_idioms.sh — passing
- 064_07_complex_array_operations.sh — passing (fixed: array assignment with `$(...)` now generates native `sort values %hash`)
- 064_22_function_returning_complex_data_structures.sh — passing
- 000__02_output_formatting_commands.sh — passing (fixed: Digest::SHA import detected for standalone assignments)
- 000__04b_file_directory_operations.sh — passing
- 000__04g_checksum_commands.sh — passing (same Digest::SHA fix)
- 000__04h_complex_examples.sh — passing (fixed: backtick command substitutions in array elements now execute via backtick syntax)
- 000__07_find_path_commands.sh — passing
- 063_15_complex_function_definition.sh — passing

## Still failing tests (2)

### 063_hard_to_parse.sh
Multiple complex issues remain:
1. The `while IFS= read -r line && [ -n "$line" ] && (( counter < max_lines ))` loop
   condition generates invalid Perl (multiple statements inside `while (...)`).
   Fixing requires rewriting how block conditions in while loops are translated.
2. Complex nested while loop with process substitution (`done < <(grep ...)`)
   generates malformed Perl code.

### 064_hard_to_generate.sh
Multiple issues:
1. Complex nested subshells with process substitution generate incorrect Perl.
2. The `((i = 1 + (2 * 3) / 4))` and `((j = i++ + ++i))` arithmetic expressions
   are not properly translated.
3. Here-documents with variable interpolation produce literal text instead of
   interpolated output.
4. Process substitution in pipelines (`paste <(...) <(...)`) is not handled.
5. Several other complex bash constructs fail to generate correct Perl.
