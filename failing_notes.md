# Failing Test Notes

## Current status: ~329/517 passed, ~188 failing

### Newly Fixed (this session):

1. **Broken `\n` in output string literals (`ir.rs`)** —
   `try_embed_newline_in_string_literal` had malformed format strings where
   `\n` (escaped backslash+n) was emitted as `\` + raw newline or as a
   literal newline, breaking Perl code like `print "start\` + newline + `";`
   instead of `print "start\n";`.  Fixed both the single-quoted and `q{...}`
   branches to use `"print \"{}\n\";\n"` (matching the already-correct
   double-quoted branch).  Fixed several tests that use `echo` with bare words.

2. **`true` builtin emitted `1;` instead of `0;` (`builtins.rs`)** —
   The if-condition handler wraps conditions in `!()` to convert shell exit
   codes (0=success) to Perl truth values (non-zero=truthy).  `true` should
   produce exit code 0 so that `!(0)` evaluates to truthy.  Changed `true`
   to emit `0;` instead of `1;`.  Fixed tests:
   `parse-heredoc-in-if.sh`, `parse-heredoc-eof-unexpected.sh`,
   `redirect-all.sh`.

3. **`Command::Not` in if-conditions had wrong negation (`control_flow.rs`)** —
   The `! cmd` handler wrapped the inner exit code in `!()` but the exit
   code already maps correctly: 0 (inner success) is falsy, 1 (inner failure)
   is truthy, matching the semantics of shell `!` (which enters then-branch
   when the inner command fails).  Removed the `!()` wrapper.  Fixed:
   `not-negation.sh`.

4. **`local` on lexical variables in subshells (`subshell_commands.rs`)** —
   `IrStmt::Declare { local: true }` emitted `local $var = $var;` which
   fails because Perl's `local` does not work on lexical (my) variables.
   Changed to `local: false` so the IR emits `my $var = $var;` instead.
   This creates a new lexical that shadows the outer one and is
   automatically restored when the block exits — the same semantics as
   a bash subshell.  Fixed: `049_local.sh`.

5. **Positional parameters `$1`, `$2`, … in case statements (`words.rs`)** —
   `word_to_perl_impl` for `Word::Variable` mapped `$1` → `"$1"` (a Perl
   variable literally named `1`), which made no sense.  Then a string hack
   in `control_flow.rs` replaced `$1` → `$arg1`, which was an undeclared
   variable causing `use strict` compilation errors.  Fixed by translating
   digit-only variable names in `word_to_perl_impl` to `$_[0]`, `$_[1]`, …
   (Perl's positional-parameter convention), and removed the fragile string
   replacement in the case-statement handler.  Fixed:
   `parse-double-semicolon-in-case.sh`.

## Remaining failures (~186)

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
