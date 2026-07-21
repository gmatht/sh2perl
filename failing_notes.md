# Failing Test Notes

## Tests fixed in this session (no remaining changes needed)

- 049_local.sh — passing (fixed: subshell local copies now initialize from outer scope)
- 051_primes.sh — passing (fixed: pre-analysis no longer skips function bodies, so `local` declarations inside functions are properly emitted)
- 055_factorize.sh — passing (same fix as 051_primes)
- 058_advanced_bash_idioms.sh — passing (fixed: `local` handler in generator always emits `my` declaration regardless of `function_level_vars`)
- 064_22_function_returning_complex_data_structures.sh — passing
- 000__04b_file_directory_operations.sh — passing
- 000__04h_complex_examples.sh — passing
- 000__07_find_path_commands.sh — passing
- 063_15_complex_function_definition.sh — passing

## Still failing tests (3)

### 063_hard_to_parse.sh
Multiple complex issues remain:
1. The `while IFS= read -r line && [ -n "$line" ] && (( counter < max_lines ))` loop
   condition generates invalid Perl (multiple statements inside `while (...)`).
   Fixing requires rewriting how block conditions in while loops are translated.
2. Complex nested while loop with process substitution (`done < <(grep ...)`)
   generates malformed Perl code.

### 064_07_complex_array_operations.sh
The test uses `IFS=$'\n' sorted=($(sort <<<"${config[*]}"))` to sort config values.
The parser does not properly handle command substitution (`$(...)`) inside array
assignments — it captures the text literally rather than parsing it as a
Word::CommandSubstitution. Fixing requires changing `parse_array_elements` to
return `Vec<Word>` instead of `Vec<String>`, or adding post-processing to
convert `$(...)` strings back into command substitutions.

### 064_hard_to_generate.sh
Multiple issues:
1. Complex nested subshells with process substitution generate incorrect Perl.
2. The `((i = 1 + (2 * 3) / 4))` and `((j = i++ + ++i))` arithmetic expressions
   are not properly translated.
3. Here-documents with variable interpolation produce literal text instead of
   interpolated output.
4. Process substitution in pipelines (`paste <(...) <(...)`) is not handled.
5. Several other complex bash constructs fail to generate correct Perl.
