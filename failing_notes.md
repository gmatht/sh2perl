# Failing Test Notes

## Current status: ~321/517 passed, ~196 failing

### Newly Fixed (this session):

1. **Function call / signature mismatch with `fn_param_names`**:
   Fixed two bugs in the parameter-name-map feature:
   - **`simple_commands.rs`**: Function calls were using named-argument
     syntax (`file => 'value'`) while function definitions used positional
     unpacking (`my ($file) = @_;`). Changed calls to use positional args
     matching the definition.
   - **`control_flow.rs`**: The removal of redundant `my $file = $_[0];`
     lines failed because the pattern expected the line without `my`.
     Updated to match both `my $x = $_[N];` and `$x = $_[N];`.
   Fixed tests: `test_simple_function`, `061_test_local_names_preserved`,
   `092_for_arith_func`, `055_factorize`.

2. **Unquoted `{`, `}`, `$` in reconstructed bash command strings**:
   The `word_to_bash_string_for_system` and `word_to_bash_string` helpers
   (and `needs_shell_quoting_literal`) were missing `{`, `}`, and `$` from
   their list of characters requiring shell quoting.  This caused awk/sed
   programs like `{print$3}` to be emitted unquoted, letting bash expand
   `$3` as a positional parameter (empty in `bash -c` context) and turning
   awk's program into `{print}` (prints whole line).
   Added the missing characters to both functions.
   Fixed test: `sqs-overlap-singleline`.

## Remaining failures (~196)

The remaining failures fall into categories that require deeper parser/generator work:

1. **Parser failures on edge cases** (~11 tests):
   - `arith-base-notation.sh`: `10#x` in arithmetic — `#` lexed as comment
   - Heredoc/apostrophe issues: apostrophe-delimited heredocs misparsed
   - Backslash continuation in `$(...)` or `[ ... ]` causing parser errors
   - These need parser-level fixes (lexer context awareness for `#`, heredoc delimiter parsing)

2. **Complex pipeline output differences**:
   Extra blank lines, missing output, or duplicated output from multi-stage
   pipelines. The pipeline generator produces slightly different results than
   bash for certain command combinations.

3. **Test expression ([[ ... ]] / test / []) translation**:
   Extglob patterns (@(...)), backslash continuations, complex groupings.
   The test expression generator doesn't handle all shell constructs.

4. **String interpolation edge cases**:
   Variables inside double-quoted strings with special characters,
   backslash escapes, and command substitutions produce different output.

5. **Array variable handling**:
   Associative arrays with shell-internal operations (${!map[@]} keys),
   array slicing, and complex array operations.

6. **`$$` comparison always fails**:
   `dollar-dollar.sh` compares PID values which are inherently different
   between bash and perl runs. This test can never pass with a plain
   stdout comparison.

7. **`$?` exit code tracking**:
   Some scripts use `$?` in ways that the generated Perl doesn't correctly
   preserve (e.g. `$? >> 8` vs raw `$?` depending on context).

8. **Backslash continuation in strings/heredocs**:
   The lexer doesn't handle backslash-newline continuations inside quoted
   strings or heredocs in all cases.

9. **Redirection handling**:
   Some redirect combinations (clobber, append, same-line heredocs) produce
   different output between Perl and bash.

10. **`set -e` and `set -u` interaction**:
    The track-every-exit-code pattern interacts poorly with the generated
    output in some corner cases.

Each of these requires targeted fixes in specific parser or generator modules.
