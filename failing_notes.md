# Failing Test Notes

## Current status

Fixed 9 additional tests this session:
- `proc-subst-output.sh` (regression)
- `parse-heredoc-eof-unexpected.sh` (regression)
- `at-var-default.sh`, `dollar-at-default-quoted.sh`, `dollar-at-with-default.sh`
- `dq-hashbang-multiline-assign.sh`
- `func-after-or-assign.sh`, `func-after-test-or.sh`
- `parse-db-status-fmt.sh`, `parse-dollar-at-default.sh`, `parse-variable-default-with-quotes.sh`

### Newly Fixed (this session):

1. **Heredoc body `${var}` not converted to Perl variable reference** —
   Unquoted heredoc bodies (`<<EOF`) were passed as raw text with `${var}`
   patterns into `IrExpr::Str(body, StrStyle::Heredoc)`.  The `Heredoc` style
   preserved `$` for Perl interpolation, so `${VAR}` became `$VAR` (undeclared
   under `use strict`).  Added `preprocess_shell_vars_in_raw_string` in
   `words.rs` that uses regex to find `${identifier}` and `$identifier` patterns
   in the raw body and converts them to `$ENV{var}` for env-style vars or
   `$var` for declared vars before the `IrExpr::Str` is created.
   Fixed: `parse-heredoc-eof-unexpected.sh`, `at-var-default.sh`,
   `dollar-at-default-quoted.sh`, `dollar-at-with-default.sh`,
   `dq-hashbang-multiline-assign.sh`, `func-after-or-assign.sh`,
   `func-after-test-or.sh`, `parse-db-status-fmt.sh`,
   `parse-dollar-at-default.sh`, `parse-variable-default-with-quotes.sh`.

2. **`bash -c` command strings use `force_interp` causing shell vars to be
    pre-interpolated by Perl** —
   Code paths in `words.rs` that generate `bash -c` fallback commands used
   `perl_string_literal_force_interp` which produces double-quoted Perl strings.
   Shell variable references like `$LOG_FILE` inside these strings were
   interpolated by Perl as the Perl variable `$LOG_FILE` (which no longer existed
   after the `$ENV{var}` conversion for uppercase vars), becoming undef.
   Changed five call sites in `words.rs` to use `perl_string_literal_no_interp`
   instead, preserving shell variable references for bash to expand.
   Fixed: `proc-subst-output.sh`.

3. **Assignment to uppercase vars uses `my $VAR` while test expr uses `$ENV{VAR}`** —
   (Previous fix preserved — kept the `$ENV{var}` emit for undeclared uppercase
   vars in assignments, but the heredoc and bash-command paths now also correctly
   handle the `$ENV{var}` format for consistency.)

### Previously Fixed (from earlier sessions):

See notes below for full list of prior fixes.

### Previously Fixed:

1. **Literal `$` at end of double-quoted string caused Perl special-variable warnings** —
   `push_string_expr` in `words.rs` escaped `"` and `@` but not `$` when
   building Perl double-quoted string literals from literal text parts.  A `$`
   at end of string (e.g. `"hello$"`) became `"hello$"` which Perl interpreted
   as the special variable `$"` or `$\`.  Added smart `$` escaping that only
   escapes `$` when NOT followed by a valid Perl identifier character.
   Fixed: `parse-dollar-end-of-string.sh`, `parse-dollar-at-end-of-line.sh`.

2. **Special shell variables `$?` and `$*` had incorrect Perl mappings** —
   `$?` (exit code) was emitted as `$?` which is Perl's 16-bit wait status;
   should be `($? >> 8)`.  `$*` (all args) was emitted as `$*` which is a
   removed Perl special variable; should be `@ARGV`.  Fixed mappings in
   `word_to_perl_impl` (`words.rs`), `perl_string_literal_impl` (`utils.rs`),
   and all three echo-handler match blocks (`simple_commands.rs` x2,
   `echo.rs` x1).  Fixed: `dollar-after-dollar.sh`.

3. **Unquoted `$` followed by non-identifier (e.g. `$//`) incorrectly split words** —
   In the parser, `Token::Dollar` was not included in the contiguous bare-word
   token merging inner loop, so `$` always created a word boundary even when
   followed by a non-identifier (like `/`).  Added `Token::Dollar` to the inner
   loop of both `parse_word` and `parse_word_no_newline_skip`.  When `$` is
   followed by a non-identifier (not a variable name), it is consumed as a
   literal character and merging continues.
   Fixed: `dollar-followed-by-slash.sh` (the `$//` merging part).

4. **Backslash in unquoted `Word::Literal` preserved instead of quote-removed** —
   In unquoted words, bash's quote removal strips backslashes before ordinary
   characters (e.g. `\.` → `.`).  Added `apply_shell_quote_removal()` that
   removes `\X` → `X` for all X, applied in `perl_string_literal_impl` before
   processing `Word::Literal`.  This avoids parser-level changes that caused
   regressions.
   Fixed: `dollar-followed-by-slash.sh` (the `\.flf` → `.flf` part).

5. **`$!` (background PID) mapped to Perl `$!` (errno) instead of empty string** —
   `$!` in bash is the PID of the last background process, which the generated
   Perl doesn't simulate.  Added `"!" => "''"` to all special-variable match
   blocks in `word_to_perl_impl` (`words.rs`), `perl_string_literal_impl`
   (`utils.rs`), `echo.rs`, and `simple_commands.rs`.  In string interpolation
   context (`convert_string_interpolation_to_perl_impl`), `$!` is silently
   omitted (empty string) since no background PID tracking exists.
   Fixed: `dollar-bang.sh`.

6. **`echo` with command substitution missing trailing newline** —
   The echo handler in `simple_commands.rs` skipped adding `\n` for command
   substitution arguments, assuming the substitution result already contained
   proper formatting.  Changed to use `IrStmt::Output` with `newline: true`,
   which adds the trailing newline that bash's `echo` always produces.
   Fixed: `008_simple_backup.sh`.

7. **`$#` inside arithmetic (`((10#x > 5))`) treated as comment by lexer** —
   `resolve_double_paren_ambiguity` in `lexer.rs` did not scan `Comment` token
   text for `)` characters, so `#x > 5))` was consumed as a comment and the
   `((` was incorrectly split into nested subshells.  Added `Comment` token
   handling to count `)` inside comment text when resolving `((` ambiguity.
   Fixed: `arith-base-notation.sh`.

8. **`N#variable` base notation in arithmetic emitted as `N#$var` (Perl comment)** —
   `convert_arithmetic_to_perl_impl` in `words.rs` left `N#` prefix in the
   output, which Perl interpreted as a comment start.  Added a preprocessing
   phase to strip bash base-notation prefixes (`\d+#`) since Perl uses base
   10 natively.  Also removed `$main_exit_code` assignment from `let` command
   generator to avoid undeclared-variable errors inside function bodies.
   Fixed: `arith-base-notation.sh` (the code-gen part).

## Remaining failures (~153)

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
