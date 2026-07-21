# Failing Test Notes

## Tests fixed in this session

### Generator: handle here-strings with tr/grep natively instead of qx{echo} fallback
Modified the command-substitution handling in `words.rs` for `Command::Redirect`
with here-strings: when the base command is `tr` or `grep`, generate native Perl
code via `generate_tr_command_for_substitution()` / `generate_grep_command()`
instead of the `qx{echo "$here_input" | cmd}` fallback. This fixes QX_BUILTIN
violations for `echo` and `tr`.

Fixed tests: 000__04h_complex_examples.sh

### Generator: fix test expression operator precedence
Reordered the `generate_test_expression_impl()` if-else chain so that logical
operators (`-a`, `-o`) are checked FIRST and recurse into their sub-expressions,
followed by NOT/grouping, then string comparison operators (`=~`, `==`, `!=`,
`=`), then numeric comparisons (`-lt`, `-le`, `-gt`, `-ge`, `-eq`, `-ne`), then
unary tests (`-z`, `-n`, `-f`, `-d`, etc.). Previously, `-ne` and `-eq` were
checked before `-a`/`-o`, causing compound conditions like
`$num -gt 3 -a $letter!="c"` to be incorrectly split on `-ne`/`!=` first,
leaving `-gt` and `-a` untranslated.

Also restored unary test checks (`-z`, `-n`, `-f`, `-d`, `-e`, etc.) that were
missing from the reordered chain.

Fixed tests: 055_factorize.sh (was `-z` not translated), partially fixes
058_advanced_bash_idioms.sh (test expression syntax errors fixed, but
matrix/declare issues remain)

## Previously fixed tests

### Generator: ls -l falls back to shell qx{} for exact output
Fixed tests: 062_10_simple_pipeline.sh, test_system_builtin.sh

### Generator: trap command translated to END block / %SIG handler
Fixed tests: 064_23_complex_error_handling_traps.sh

### Generator: arithmetic expressions wrapped in eval {} // "" for div by zero
Fixed tests: 063_01_deeply_nested_arithmetic.sh

### Generator: handle `${var[@]:offset:length}` array slicing and env var dependency ordering
Fixed tests: 064_18_array_slicing_manipulation.sh

### Generator: support `Nested` and `Compound` BraceItem variants
Fixed tests: 064_12_brace_expansion_nested_sequences.sh

### Generator: sort associative array values for deterministic ${map[@]} expansion
Fixed tests: 064_02_nested_brace_expansions.sh (and others via `sort values %map`)

### Generator: handle `$(...)` inside arithmetic expressions
Fixed tests: 064_14_nested_command_substitution_arithmetic.sh

### Generator: fix `\$` escaping in double-quoted strings for eval
Fixed tests: partially fixes 063_12_complex_eval.sh

### Generator: use `scalar(keys %map)` for `${#map[@]}` on associative arrays
Fixed tests: partially fixes 063_09_complex_function_parameter_handling.sh

## Still failing tests (12)

### 058_advanced_bash_idioms.sh
Complex script combining many feature interactions including `declare -A`
(associative arrays) with matrix-like `$matrix[$i,$j]` access (multidimensional
syntax not supported in Perl), `let` commands passed to system() which hangs,
and heredoc-with-subshell translation issues.

### 062_hard_to_lex.sh
Variable name collision: `$result` in function body refers to global
`$result` (value 776) instead of the local `$result_273` created by the
command-substitution translation for `` `echo "$param1" | sed "s/old/new/g"` ``.
The `local result=$(cmd)` pattern generates code where the assignment target
is a different variable than the one used later.

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
