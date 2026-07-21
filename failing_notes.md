# Failing Test Notes

## Tests fixed in this session

### 063_09_complex_function_parameter_handling.sh (FIXED)
Fixed `${var#pattern}` prefix removal on array elements (e.g. `${args[i]#--}`):
`parse_parameter_expansion_content` now checks for operators like `#`, `##`,
`%`, `%%`, `^`, `^^`, `,,` in the `rest` portion after the closing `]` before
returning array access.  The generator in `words.rs` now applies these operators
to the array access expression (e.g. `($args[...] =~ s/^--//r)`).

### 064_09_process_substitution_pipeline.sh (FIXED)
Fixed two issues:
1. `cut -d: -f1 /etc/passwd` inside a process substitution pipeline now reads
   from the file directly instead of using empty pipeline input (`$;` syntax error).
   Added `generate_cut_command_with_output()` that handles file arguments and
   empty input_var by reading from files or STDIN, and writes to output_var.
2. `paste` with process substitution inside a pipeline now sets `$output` for
   pipeline chaining only when inside a pipeline (detected via
   `current_pipeline_output_id()`), preventing undeclared variable errors in
   standalone paste usage.

### 064_22_function_returning_complex_data_structures.sh (PARTIALLY FIXED)
- Fixed `declare -p` to generate Perl code that prints the hash in
  bash-compatible `declare -A name=([key]="value" ...)` format.
- Fixed IPC::Open3 detection to scan function bodies and BuiltinCommand
  arguments, so `use IPC::Open3;` is correctly included in the preamble.
- Remaining issue: key order in output differs from bash's hash bucket order.

### QX/OPEN3 violations (FIXED)
Fixed multiple QX violations where generated Perl used `qx{}`, `system()`, or
`open3()` with builtin commands (cp, mkdir, find, wc) instead of native Perl:

1. **cp.rs**: Replaced `qx{$cp_cmd}` fallback with native `File::Copy::copy`
   implementation in `generate_cp_command()`.
2. **find.rs**: Replaced `qx{$command}` and `open3('find', ...)` with native
   `File::Find` implementation that handles `-name`, `-type`, `-maxdepth` flags.
3. **words.rs (mkdir)**: Replaced `qx{$mkdir_cmd}` fallback with native
   `generate_mkdir_command()` in command substitution handler.
4. **words.rs (wc redirect)**: Added native Perl implementation for `wc` with
   input redirect (`wc -c < file`), replacing the `qx{$command}` fallback.

### test_simple_function.sh (FIXED)
Fixed wc with input redirect (`wc -c < "$file"`) in command substitution:
the native wc handler now returns the count without a trailing newline,
matching bash's command substitution behavior (stripping trailing newlines).

## Still failing tests (9)

### 000__04b_file_directory_operations.sh
Uses `ls -a` and `find . -name "*.sh" -type f` in backtick command
substitution. The native `ls` and `File::Find` implementations produce
output in sorted order, which differs from the real `find`/`ls` traversal
order. Hard to fix without maintaining exact filesystem iteration order.

### 000__04h_complex_examples.sh
Complex script with multiple backtick commands: nested backticks, pipelines
with wc, functions, arrays from command substitution, process substitution,
here-strings, and perl command. Multiple features interact in ways that
produce incorrect output.

### 000__07_find_path_commands.sh
Uses `find . -name "*.sh" -type f` in backtick command substitution.
Same ordering issue as 000__04b: File::Find sorted output differs from
real find's depth-first traversal order.

### 058_advanced_bash_idioms.sh
Uses `find . -maxdepth 1 -type f | wc -l` and `find . -maxdepth 1 -type d | wc -l`
in command substitution. The native find and wc implementations may produce
different output formatting or ordering than the real commands.

### 063_12_complex_eval.sh (FIXED)
Fixed `${var:-0}` parameter expansion default-value syntax inside `$(( ... ))`
arithmetic expressions. Added `convert_param_expansion_in_arith()` and a
pre-processing phase in `convert_arithmetic_to_perl_impl()` that handles
`${var:-default}`, `${var:=default}`, `${var:+default}`, and
`${array[key]:-default}` patterns before the general variable conversion.
Also fixed auto-declaration to properly detect array variables (`my @array`
instead of `my $array`) and use `$array[$key]` (indexed) vs `$array{$key}`
(associative) access based on `associative_arrays`.

### 063_15_complex_function_definition.sh
`eval "$name() { $body }"` — eval with dynamic arguments (using function
parameters `$name` and `$body`) is not supported. The eval handler now
generates code that runs the eval via `system('bash', '-c', ...)` at runtime,
but the function definition is lost after the bash process exits, so
subsequent calls to the dynamically-defined function fail.

### 063_hard_to_parse.sh
Multiple parsing issues: complex nested parameter expansions, `${!prefix@}`
expansion, eval inside function definitions, deeply nested arithmetic
expressions, here-documents with complex content, etc.

### 064_07_complex_array_operations.sh
Perl hash randomization causes `values %config` to return values in
random order. The generator currently uses `sort values` which sorts
values alphabetically (`8080 admin localhost`) but bash outputs in hash
bucket order (`8080 localhost admin`). Fix requires maintaining the
original key insertion order and iterating in that order instead of
using `sort values %`. This is hard because the AST stores env_vars
in a BTreeMap (sorted by key), losing source insertion order.

### 064_22_function_returning_complex_data_structures.sh (remaining issue)
The `declare -p info` output uses `sort keys %info` which sorts keys
alphabetically, but bash outputs in hash bucket order. Same root cause
as 064_07.

### 064_hard_to_generate.sh
Complex script combining many hard-to-generate features: eval (dynamic),
declare -p (order mismatch), process substitution with nested pipelines,
nested command substitution in arithmetic, here-documents with variable
interpolation, extended glob patterns, complex function definitions, etc.
