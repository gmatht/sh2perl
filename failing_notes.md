# Failing Test Notes

## Tests fixed in this session

### Generator: fix `let` command exit-code pollution inside `if` conditions
Added a separate match arm in `generate_if_statement_impl` for `Command::Simple`
with name "let" that generates the arithmetic expression directly without the
`$main_exit_code =` assignment. Previously the `let` handler in
`simple_commands.rs` always emitted `$main_exit_code = eval { int(EXPR) } // "";`,
and when this was used as an `if` condition, the assignment set the program's
exit code to 1 (or the condition's truth value), causing `exit $main_exit_code`
to exit with code 1 instead of 0.

Fixed tests: 058_advanced_bash_idioms.sh (exit code now matches shell)

### Generator: fix `dirname "$(pwd)"` nested command substitution quoting
Fixed `word_to_bash_string_for_system` in `system_commands.rs` to properly
handle `StringPart::CommandSubstitution` parts inside `StringInterpolation`.
The wildcard `_ =>` arm was pushing literal `$var` as a placeholder for
command substitution parts. Added explicit arms for `CommandSubstitution`
and `Arithmetic` variants. Also fixed double-quote escaping to preserve
quotes inside `$(...)` and `${...}` constructs rather than escaping them
with backslashes, which caused nested command substitutions like
`dirname "$(dirname "$(pwd)")"` to produce incorrect shell commands.

Fixed tests: 058_advanced_bash_idioms.sh (dirname output now matches shell)

### Generator: save/restore declared_locals/function_level_vars around function bodies
Added save/restore of `declared_locals`, `function_level_vars`, and
`associative_arrays` in `generate_function_impl` so that variables declared
via `local` inside a function don't leak into the outer scope (and vice
versa, outer scope vars don't conflict with function-internal declarations).
Previously all `local` declarations were added to the global sets, causing
Perl `my $var` declarations to be skipped when the same variable name was
used later in the main script body.

### Generator: fix env-var fallback path to scope variables per command
Changed the `_ =>` fallback in `word_to_perl_impl` (And/Or command handling)
to collect only the shell variables actually referenced by the bash command
(via `collect_shell_vars_from_command`) instead of blindly iterating over
the entire `declared_locals` set.  The old code caused `local $ENV{var} =
$var;` lines for variables that were out of lexical scope in the generated
Perl, leading to "Global symbol requires explicit package name" errors.

### Generator: fix `$ENV{var}` to use `// ""` for undeclared variables
Changed `parameter_var_scalar_ref` and `parameter_var_bare_ref` in
expansions.rs to produce `($ENV{var} // "")` instead of bare `$ENV{var}`
for undeclared variables.  In bash, reading an undeclared variable yields
empty string; in Perl, `$ENV{var}` returns `undef` when the environment
variable is not set, triggering "uninitialized value" warnings under
`use warnings`.

### Generator: fix hash key with comma in associative array access
When a MapAccess key contains a comma (e.g. `${matrix[$i,$j]}`), the
generated Perl `$matrix{$i,$j}` is interpreted as the comma operator
(equivalent to `$matrix{$j}`).  The fix detects commas in the key and
emits `$matrix{"$i,$j"}` (double-quoted interpolation) instead.

### Generator: fix `let` condition negation in `if` statements
The `if` statement generator unconditionally negated the condition for
non-trivial commands (to convert bash exit-code conventions to Perl
truth values).  For `let` commands, which already produce a 1/0 value
from `eval { int(...) }`, the extra negation flipped the logic (even
numbers printed "Odd" and vice versa).  Added a special case to skip
negation for `let` commands.

Fixed tests: 058_advanced_bash_idioms.sh (several remaining issues
listed below), 064_19_complex_pattern_matching_extended_globs.sh

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

### Generator: handle array-index env-var assignment syntax
When a standalone assignment with an array subscript (e.g. `matrix[0,2]=3`)
is parsed as an environment variable for the next command, the generator now
recognises the `var[key]` pattern in the env-var name and emits `$var{key}`
syntax instead of the invalid `my $var[key]` syntax, which Perl rejects as
multidimensional array access.

Partially fixes: 058_advanced_bash_idioms.sh (matrix assignments no longer
cause a Perl syntax error)

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

## Tests fixed in this session (continued)

### 064_19_complex_pattern_matching_extended_globs.sh
Now passes. The `${i,$j}` comma-key fix and other enhancements resolved
this test.

## Still failing tests (8)

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

### 064_22_function_returning_complex_data_structures.sh
`declare -A info` and `declare -p info` not translated. Bash's
`declare -p` prints variable metadata; the generator emits `$info[`
which is a syntax error.

### 064_hard_to_generate.sh
Complex script combining many hard-to-generate features: eval, declare
-p, process substitution, nested command substitution in arithmetic, etc.
