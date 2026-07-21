# Failing Test Notes

## Tests fixed in this session

### 063_09_complex_function_parameter_handling.sh (FIXED)
Fixed `${var#pattern}` prefix removal on array elements (e.g. `${args[i]#--}`):
`parse_parameter_expansion_content` now checks for operators like `#`, `##`,
`%`, `%%`, `^`, `^^`, `,,` in the `rest` portion after the closing `]` before
returning array access.  The generator in `words.rs` now applies these operators
to the array access expression (e.g. `($args[...] =~ s/^--//r)`).

## Still failing tests (7)

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
`declare -p` prints variable metadata; the generator needs to emit
Perl code that prints the hash in `declare -p` format.

### 064_hard_to_generate.sh
Complex script combining many hard-to-generate features: eval, declare
-p, process substitution, nested command substitution in arithmetic, etc.
