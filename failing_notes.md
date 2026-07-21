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
Partially fixed:
- Fixed: `${index}` in array index context was treated as Perl builtin `index`
  instead of variable `$ENV{index}` (removed `"index"` from Perl builtins list
  in `convert_arithmetic_to_perl_impl`).
- Fixed: While loop with env-var prefix (`IFS= read -r line && ...`) was wrapped
  in a `Block` by the parser, causing invalid `while (my $IFS = q{}; ...)` code.
  Added `Block` handling in `generate_while_loop_impl`.
- Fixed: `$(...)` command substitutions inside test expression operands (e.g.
  `[ "$(wc -l < "$file")" -gt 10 ]`) now convert to `qx{...}`.

Remaining issues:
1. Process substitution redirect (`done < <(cmd)`) writes to a temp file but
   the while body's `read` command reads from STDIN instead of the temp file.
2. Background commands via `fork()`/`exec()` may not work correctly in all cases.
3. Several features like `trap`, `shopt`, `eval` with complex expansions, and
   brace expansion with ranges produce incorrect or incomplete Perl.
4. Exit code is 255 instead of 0 due to failing `system()` calls for
   unrecognized commands (translation of function call arguments).

### 064_hard_to_generate.sh
Partially fixed:
- Fixed: `$(...)` command substitutions inside test expression operands now
  convert to `qx{...}`.

Remaining issues:
1. Complex nested subshells with process substitution generate incorrect Perl.
2. The `((i = 1 + (2 * 3) / 4))` and `((j = i++ + ++i))` arithmetic expressions
   are not properly translated.
3. Here-documents with variable interpolation produce literal text instead of
   interpolated output.
4. Process substitution in pipelines (`paste <(...) <(...)`) is not handled.
5. Several undeclared variable errors (`Global symbol "$tmp_redirect_3" requires
   explicit package name`) due to incorrect variable scoping in generated code.
6. Nested function definitions (`inner_func` inside `outer_func`) produce
   "will not stay shared" warnings and may not capture variables correctly.
