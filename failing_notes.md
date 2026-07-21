# Failing Test Notes

## Tests fixed in the latest session

### 000__04b_file_directory_operations.sh (FIXED)
Fixed by removing `sort @find_results` in the find command substitution
(`generate_find_for_substitution` and `generate_find_command`). File::Find's
depth-first traversal order now matches bash's find output more closely
than sorted order.

### 000__04h_complex_examples.sh (FIXED)
Fixed two issues:
1. **wc croak → warn**: Changed `open ... or croak` to `if (open ...) { ... } else { warn ... }`
   in the wc command substitution handler so that the script doesn't die when
   a referenced file doesn't exist (matching bash behavior: error goes to stderr,
   command substitution returns empty string).
2. **Return empty string for missing file**: Added `$wc_file_opened` flag and
   ternary `$wc_file_opened ? do { ... } : q{}` so that when the file can't be
   opened, the command substitution evaluates to empty string (matching bash),
   not `0`.

### 000__07_find_path_commands.sh (FIXED)
Same fix as 000__04b: removed `sort @find_results` in find command substitution
so that File::Find's traversal order matches bash's find order.

### 058_advanced_bash_idioms.sh (FIXED)
Fixed the `-maxdepth` flag in find command substitution: moved the maxdepth
condition from outside the `File::Find::find` callback to inside it. Previously
the maxdepth check was placed before the `find()` call (returning from the do
block before find even ran), causing it to find all directories in the entire
tree instead of only top-level ones.

### 063_12_complex_eval.sh (FIXED - earlier session)
Fixed `${var:-0}` parameter expansion default-value syntax inside `$(( ... ))`
arithmetic expressions. Added `convert_param_expansion_in_arith()` and a
pre-processing phase in `convert_arithmetic_to_perl_impl()` that handles
`${var:-default}`, `${var:=default}`, `${var:+default}`, and
`${array[key]:-default}` patterns before the general variable conversion.
Also fixed auto-declaration to properly detect array variables.

### 063_15_complex_function_definition.sh (PARTIALLY FIXED)
Fixed Perl syntax error: added missing semicolon after `do { ... }` block
in the eval handler (`redirects.rs`). The generated Perl no longer has a
compilation error. However, the dynamic function definition via eval still
cannot work correctly: `system('bash', '-c', "eval ...")` defines the function
in a subprocess that exits, so subsequent calls to the dynamically-defined
function (`system('test_func')`) fail because the function no longer exists.

## Still failing tests (5)

### 063_15_complex_function_definition.sh
`eval "$name() { $body }"` — eval with dynamic arguments (using function
parameters `$name` and `$body`) cannot define a persistent function because
the eval runs in a subprocess via `system('bash', '-c', ...)`. The function
definition is lost after the bash process exits, so subsequent calls to the
dynamically-defined function fail. A proper fix would require intercepting
the eval pattern and generating a real Perl subroutine at compile time.

### 063_hard_to_parse.sh
Multiple parsing and generation issues: undeclared variables (`$file`, `@array`),
syntax errors in generated code (`@]}`), problems with `head` command arguments,
and other complex edge cases in deeply nested bash constructs.

### 064_07_complex_array_operations.sh
Perl hash randomization causes `values %config` to return values in
random order. The generator uses `values %config` (previously `sort values`)
but neither matches bash's hash bucket order. Bash outputs in the order
`8080 localhost admin` while Perl outputs in a different order.
Fix requires maintaining the original key insertion order and iterating
in that order instead of using `values %`. Hard because the AST stores
env_vars in a BTreeMap (sorted by key), losing source insertion order.

### 064_22_function_returning_complex_data_structures.sh
The `declare -p info` output uses `keys %info` which iterates keys in
Perl's internal hash order, but bash outputs in hash bucket order.
Same root cause as 064_07: hash iteration order differs between Perl
and bash.

### 064_hard_to_generate.sh
Complex script combining many hard-to-generate features: eval (dynamic),
declare -p (order mismatch), process substitution with nested pipelines,
nested command substitution in arithmetic, here-documents with variable
interpolation, extended glob patterns, complex function definitions, etc.
Multiple undeclared variable errors and syntax issues in generated code.
