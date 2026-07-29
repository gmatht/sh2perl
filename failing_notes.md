# Failing Test Notes

## Current status: ~311/517 passed, ~206 failing

Systematic issues that have been FIXED:

1. **`$$` (process ID) converted to `$ENV{$}` instead of `$$`**:
   Fixed in `words.rs` by adding `"$"` to the special variable lists in both
   `word_to_perl_impl` and `convert_string_interpolation_to_perl_impl`.

2. **Double newline from command substitution**:
   `cmd_str_to_open_perl` and `expr_to_open_perl` now chomp the captured
   output, matching shell command-substitution semantics. Also fixed the
   `local $/` race with `chomp` (chomp must execute after `$/` is restored
   to its default value).

3. **`! negation` wrapping for standalone commands**:
   Changed from `!(...)` to `!do { ... };` so multiple statements (variable
   declarations, etc.) inside the negation are valid Perl.

4. **`! negation` in if-conditions**:
   The `Command::Not` handler in `control_flow.rs` now applies `!()` to the
   inner condition, correctly flipping shell exit-code semantics.

5. **Missing `$CHILD_ERROR` declaration**:
   `our $CHILD_ERROR;` is now emitted unconditionally since many command
   generators (grep, ls, etc.) use `$CHILD_ERROR` internally even without
   explicit command substitution in the shell source.

6. **Filehandle syntax in `Output` IR node**:
   Changed from `print {*fh} ...` to `print {$fh} ...` (scalar filehandle,
   not typeglob).

7. **`[` (test bracket) not dispatched to test handler**:
   Added `"["` to the standalone command dispatch table in
   `simple_commands.rs` alongside `"test"`.

8. **`Command::Not` missing from `cmd_has_cmdsub` traversal**:
   Added `Command::Not` to the match in `cmd_has_cmdsub` so the inner
   command's command substitutions are detected.

## Remaining failures (~206)

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
