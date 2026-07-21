# Failing Test Notes

## Tests fixed in this session

### Generator: fix `local var=$(cmd)` when `var` already exists globally
Changed the `local` handler in `redirects.rs` to use `function_level_vars`
instead of `declared_locals` for the skip-already-declared check.  `local`
always creates a new Perl `my $var` that shadows the global, so a variable
that exists at global scope must still be declared as a new local inside
the function.

Also applied the same fix in `simple_commands.rs` for consistency.

Fixed tests: 062_hard_to_lex.sh

### Generator: handle `let` (arithmetic evaluation) builtin natively
Added `let` to the builtins registry and added a handler that converts
each argument to a Perl arithmetic expression via
`convert_arithmetic_to_perl`.  Also enhanced
`convert_arithmetic_to_perl_impl` to handle bash array-length syntax
`${#var[@]}` → `scalar(@var)` and string-length `${#var}` → `length($var)`,
using placeholders to protect them from the variable-name converter.
Added Perl builtin keyword detection so identifiers like `scalar`,
`length`, `keys` are not erroneously converted to `$ENV{...}`.

Fixed tests: 063_09_complex_function_parameter_handling.sh (no longer
times out; still has pre-existing `--flag1` argument loss)

### Generator/parser: array index keys with commas, ${!prefix@} indirect expansion, and undeclared vars
Fixed the `parse_index_suffix` function to use the full text of the `TestBracket`
token (which can include extra characters due to logos fallback behavior when
`CasePattern` partially matches `[0,`).  Fixed `${!prefix@}` parsing in both the
`DollarBrace` and `DollarBraceBang` branches to produce `MapKeys` instead of
`Variable("!prefix@")`.  Updated `perl_string_literal_impl` (and its sibling
functions) to check `declared_locals`/`function_level_vars` before emitting
`$array[idx]`, `keys %hash`, or `scalar(@arr)`; undeclared variables now safely
produce `""` or `0`, matching bash's undefined-variable semantics.

Fixed tests: 063_02_complex_array_assignments.sh

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

## Still failing tests (10)

### 058_advanced_bash_idioms.sh
Complex script combining many feature interactions including `declare -A`
(associative arrays) with matrix-like `$matrix[$i,$j]` access (multidimensional
syntax not supported in Perl), `let` commands passed to system() which hangs,
and heredoc-with-subshell translation issues.

### 063_09_complex_function_parameter_handling.sh
`let` is now handled natively (no more timeout). The remaining failure is
due to a pre-existing parser bug: the `--flag1` argument in the function
call `complex_function --flag1 --option1=value1 -abc` is lost during
tokenisation (double `--` → two `Minus` tokens, the first bare `-` may
be dropped by the argument parser).

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
