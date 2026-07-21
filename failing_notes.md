# Failing Test Notes

## Tests fixed in this session

### Generator: ls -l falls back to shell qx{} for exact output
When `ls -l` (or `ls -la`) is used in pipeline or backtick context, the
generator now falls back to `qx{ls ...}` shell execution to produce exact
`ls -l` output (permissions, owner, group, size, date). The native ls
translation only produces short `d .` / `- .` prefixes which don't match
the expected full-format output.

This was already allowed by `allowed_qx_calls.txt` which lists `ls -l`
as a permitted shell fallback prefix.

Fixed tests: 062_10_simple_pipeline.sh, test_system_builtin.sh

### Generator: trap command translated to END block / %SIG handler
Added `trap` builtin handler in `generate_builtin_command_impl()`. For
EXIT traps, generates `END { system 'handler'; }`. For other signals
(INT, TERM, etc.), generates `$SIG{SIGNAL} = sub { system 'handler'; };`.
The handler is executed via shell (`system`) since translating arbitrary
shell commands to Perl is not practical.

Fixed tests: 064_23_complex_error_handling_traps.sh

### Generator: arithmetic expressions wrapped in eval {} // "" for div by zero
Changed `convert_arithmetic_to_perl_impl()` to wrap arithmetic expressions
in `eval { int(expr) } // ""` instead of bare `int(expr)`. This handles
division/modulo by zero gracefully: instead of Perl's fatal "Illegal
modulus zero", the eval catches the error and returns empty string,
matching bash's behavior of leaving the variable unset on arithmetic
failure.

Fixed tests: 063_01_deeply_nested_arithmetic.sh

## Previously fixed tests

### Generator: handle `${var[@]:offset:length}` array slicing and env var dependency ordering (qd. 2026-07-21)
Fixed tests: 064_18_array_slicing_manipulation.sh

### Generator: support `Nested` and `Compound` BraceItem variants (qd. 2026-07-21)
Fixed tests: 064_12_brace_expansion_nested_sequences.sh

### Generator: sort associative array values for deterministic ${map[@]} expansion (qd. 2026-07-21)
Fixed tests: 064_02_nested_brace_expansions.sh (and others via `sort values %map`)

## Tests fixed in this session

### Generator: handle `$(...)` inside arithmetic expressions
Modified `convert_arithmetic_to_perl_impl()` to scan for `$(...)` command
substitutions within the arithmetic expression text, replace each with a
Perl `qx{}` call that captures stdout, then process the remainder normally.

Fixed tests: 064_14_nested_command_substitution_arithmetic.sh

### Generator: fix `\$` escaping in double-quoted strings for eval
Added handling for `\$` (escaped dollar sign) inside double-quoted string
content in `parse_string_interpolation()`. Previously, `\$` was not
recognized as an escaped literal `$`, causing the parser to interpret
`$((` and `${` as real expansions inside eval arguments.

Fixed tests: partially fixes 063_12_complex_eval.sh (the `\$` is now
properly treated as literal, but the eval handler still cannot parse
the complex nested parameter expansions in the resulting command).

### Generator: use `scalar(keys %map)` for `${#map[@]}` on associative arrays
Modified both `generate_parameter_expansion_impl()` (expansions.rs) and
`convert_string_interpolation_to_perl_impl()` (words.rs) to check whether
the array name is registered as an associative array. If so, `${#arr[@]}`
generates `scalar(keys %arr)` instead of `scalar(@arr)`, which avoids
"Global symbol \"@arr\" requires explicit package name" errors.

Fixed tests: partially fixes 063_09_complex_function_parameter_handling.sh
(the `@options` vs `%options` compilation error is fixed; the test still
times out due to `let` commands being passed to `system()` which hangs).

## Still failing tests (12)

### 058_advanced_bash_idioms.sh
Complex script combining many feature interactions. Compile error: `-gt`
syntax error (test expression translation issue). Multiple other issues.

### 062_hard_to_lex.sh
Variable name collision: `$result` in function body refers to global
`$result` (value 776) instead of the local `$result_273` created by the
command-substitution translation for `` `echo "$param1" | sed "s/old/new/g"` ``.

### 063_02_complex_array_assignments.sh
Several issues: (1) `$index` undeclared, (2) `$!prefix@` bareword from
`${!prefix@}` expansion not translated correctly, (3) `matrix[0,0]` key
loses leading `0` (parsed as `,0`), (4) `@array` undeclared.

### 063_09_complex_function_parameter_handling.sh
`let` command (bash builtin for arithmetic) is passed to `system('let', ...)`
which does not exist as an external command. The while-loop condition
`system('let', 'i < ${#args[@]}')` returns a non-zero exit code (command
not found), which is truthy in Perl, causing an infinite loop / timeout.
The `@options` compilation error is now fixed (uses `keys %options`).

### 063_12_complex_eval.sh
`eval "result=\$(( \${var:-0} + ... ))"` — the `\$` is now properly treated
as literal `$`, and the eval string is correctly extracted as
`result=$(( ${var:-0} + ${array[${index:-0}]:-0} ))`. However, the parser
cannot parse this command due to the deeply nested parameter expansion
`${array[${index:-0}]:-0}` (array access with default value and nested
variable index). The eval handler falls through to the "could not parse"
comment, so `$result` remains undefined.

### 063_15_complex_function_definition.sh
Unmatched right curly bracket caused by incorrect brace/block generation
when `eval` inside a function body is not supported.

### 063_hard_to_parse.sh
Multiple parsing issues similar to 063_02: `$index` undeclared,
`$!prefix@` bareword from `${!prefix@}` expansion.

### 064_07_complex_array_operations.sh
Perl hash randomization causes `values %config` to return values in
random order. Current fix uses `sort values` but sorted order
(`8080 admin localhost`) differs from bash's hash-bucket order
(`8080 localhost admin`). Fix requires maintaining key insertion order
and iterating in that order instead of using `values %`.

### 064_09_process_substitution_pipeline.sh
Syntax error near `$;` — process substitution temp files not correctly
created/passed.

### 064_19_complex_pattern_matching_extended_globs.sh
The brace expansion `*.{txt,log,dat}` now correctly expands to
`*.txt`, `*.log`, `*.dat` in the for-loop (parser now merges the `*.`
prefix with the adjacent brace expansion; generator now includes the
prefix when emitting for-loop items). However, the for-loop still needs
glob expansion: bash expands `*.txt` to matching filenames, but the
generated Perl iterates over the literal pattern strings. Adding glob
expansion to for-loop items would fix this.

### 064_22_function_returning_complex_data_structures.sh
`declare -A info` and `declare -p info` not translated. Bash's
`declare -p` prints variable metadata; the generator emits `$info[`
which is a syntax error.

### 064_hard_to_generate.sh
Complex script combining many hard-to-generate features: eval, declare
-p, process substitution, nested command substitution in arithmetic, etc.
