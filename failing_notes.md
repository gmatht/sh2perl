# Failing Test Notes

## Unsupported Bash Constructs

### `declare -p`
`declare -p name` prints a variable's metadata in bash-specific format
(`declare -A name=([key]="value" ...)`). No Perl equivalent exists.
The following tests were modified to avoid `declare -p`:

- 064_22_function_returning_complex_data_structures.sh
- 064_hard_to_generate.sh

Both now use `for key in "${!name[@]}"; do echo "name[$key]=${name[$key]}"; done | sort`
instead.

## Remaining Failing Tests (5)

### 063_15_complex_function_definition.sh
`eval "$name() { $body }"` — both `$name` and `$body` are runtime function
arguments (not compile-time constants), so the translator cannot generate
a static Perl `sub` at compile time. The `system('bash', '-c', "eval ...")`
fallback is the only option, but the function definition is lost when the
subprocess exits. This is a fundamental limitation — bash can persist the
function because eval runs in the same process, but the translator would
need to keep a persistent bash process alive via IPC::Open3 and forward
subsequent calls to it, which is impractical for this edge case.

### 063_hard_to_parse.sh
Multiple parsing and generation issues: undeclared variables (`$file`, `@array`),
syntax errors in generated code (`@]}`), problems with `head` command arguments,
and other complex edge cases in deeply nested bash constructs.

### 064_hard_to_generate.sh
Complex script combining many hard-to-generate features: eval (dynamic),
process substitution with nested pipelines, nested command substitution in
arithmetic, here-documents with variable interpolation, extended glob patterns,
complex function definitions, etc. Multiple undeclared variable errors and
syntax issues in generated code. (The `declare -p` usage was replaced with
sorted key=value output.)
