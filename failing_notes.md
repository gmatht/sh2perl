# Failing Test Notes

## Current status: fixed func-after-test-or.sh, func-after-or-assign.sh

### Newly Fixed (this session):

1. **`${VAR}` in test expressions not converted to Perl variable references** —
   The operator-specific handlers in `generate_test_expression_impl` (like `-n`,
   `-z`, `-f`, etc.) extracted raw text from the expression and used it directly
   as Perl code.  A fragment like `"${VAR}"` was emitted verbatim, causing Perl
   to interpret `${VAR}` as the variable `$VAR` — which was undeclared because
   the pre-analysis skips all-uppercase names (they are assumed to be env vars).
   Added `preprocess_brace_vars_in_test_expr` that runs before operator-specific
   handlers and converts simple `${identifier}` patterns to the appropriate Perl
   reference (`$ENV{var}` for uppercase names, `$var` for lowercase/declared).
   Fixed: `func-after-test-or.sh`, `func-after-or-assign.sh`.

2. **Assignment to uppercase vars uses `my $VAR` while test expr uses `$ENV{VAR}`** —
   The pre-analysis skips uppercase variable names, so they are not in
   `function_level_vars`.  When an assignment like `VAR="default"` was generated,
   `needs_decl_for_assign` was true, producing `my $VAR = "default"` (a local
   lexical).  But the test expression condition used `$ENV{VAR}` — two different
   variables.  Added a check in `generate_assignment`: when the variable name is
   all-uppercase and not declared, emit `$ENV{VAR} = value` instead of
   `my $VAR = value;`.  Fixed: `func-after-or-assign.sh` (the value now persists
   to the function body, which also uses `$ENV{VAR}` via string-interpolation
   fallback for undeclared vars).

3. **`${var-}` (default-value operator without colon) not recognized by parser** —
   The main `DollarBrace` handler in `parser/words.rs` and the `DollarBraceAt`
   handler only recognised colon-prefixed operators (`:-`, `:=`, `:+`, `:?`)
   but missed the no-colon forms (`-`, `+`, `?`, `=`).  Content like
   `${ZSH_VERSION-}` was treated as a literal variable name `ZSH_VERSION-`
   (with trailing dash) instead of as parameter expansion with
   `DefaultValue("")`.  Added checks for `-`, `+`, `?`, `=` without colon
   right before the fallback-to-plain-variable `else` clause in both places.

2. **Test expression generator turns `${var-}` into literal `"${var-}"` Perl string** —
   The `generate_test_expression_impl` function's default case passed raw
   `TestExpression.expression` text through as a string literal.  Added
   `convert_shell_param_expansion_in_test_expr` that parses `${...}` patterns
   with `parse_parameter_expansion_content` and emits proper Perl
   `$ENV{var}` / `defined` checks.  Also added `test_expr_var_ref` helper
   to choose between local `$var` and `$ENV{var}` based on variable name patterns.

3. **`convert_test_args_to_expression_impl` drops `ParameterExpansion` operator info** —
   When rebuilding a Perl expression from `StringPart::ParameterExpansion`, the
   old code only used `pe.variable` (e.g. `${ZSH_VERSION}`) and ignored the
   operator entirely.  Now handles `DefaultValue` (with and without non-empty
   default) and other operators, using `test_expr_var_ref` for the variable reference.

4. **`analyze_test_expression_vars` includes `-` in extracted variable names** —
   The pre-analysis scan that extracts `${...}` variable names from raw test
   expression strings only stripped `:`, `#`, `%`, `/`, `!`, `^`, `,` from
   variable names but missed `-`, `+`, `?`, `=`.  `ZSH_VERSION-` was declared
   as `my $ZSH_VERSION-;` — a Perl syntax error (hyphen not valid in identifiers).
   Added the missing characters to the split set.

5. **`analyze_test_expression_vars` declares all-uppercase env-style vars as local** —
   Variables like `BASH_VERSION`, `ZSH_VERSION` from test expressions were added
   to `function_level_vars` and later declared as `my $ZSH_VERSION;` (undef).
   These should use `$ENV{var}` instead.  Added a skip for all-uppercase names
   (matching `/^[A-Z_]+$/`).

6. **Race condition on `__tmp_run.pl` temp file in parallel test execution** —
   The `debashc` binary used a hard-coded temp file name `__tmp_run.pl` when
   running generated Perl code.  When the `./fail` script runs tests in parallel
   (4 workers), multiple `debashc` processes race on this file — one writes it,
   another overwrites it, a third deletes it before perl can execute.  This
   corrupts output and causes cascading stdout mismatches.  Fixed by using
   `format!("__tmp_run_{}.pl", std::process::id())` (PID-unique) in all four
   code paths that write `__tmp_run.pl`.  Fixed: `008_simple_backup.sh`,
   `062_10_simple_pipeline.sh`.

2. **`))` consumed but `get_current_text()` called after `next()` in commands.rs ParenClose handler** —
   `parse_arithmetic_expression` in `commands.rs` called `self.lexer.next()` BEFORE
   `self.lexer.get_current_text()` for `ParenClose` tokens, losing the `)` character
   and pushing the next token's text instead.  Fixed by moving `next()` after
   `get_current_text()`.  Fixed: `078_arithmetic_double_paren.sh`,
   `064_06_nested_arithmetic_expressions.sh`.

3. **`print($expr, "\n")` with parenthesized expression broken by space before `(`** —
   `emit_stmt` in `ir.rs` used `"print {}, \"\\n\";\n"` format which produced
   `print (EXPR), "\n"` — Perl parsed the space before `(` as `print` with one
   parenthesized argument, then the `, "\n"` was in void context causing warnings
   and wrong output.  Changed to `"print({}, \"\\n\");\n"` (and likewise for
   no-newline and filehandle cases).  This fixes `unicode-in-string.sh` and
   any other test where print arguments start with `(`.

4. **Extra `))` in arithmetic expressions with nested parens** —
   `parse_arithmetic_expression` (in `words.rs`, `assignments.rs`, and `commands.rs`)
   pushed closing parens to the expression even when they closed the outer `$((` / `((`
   marker.  Fixed three copies of this function so that `ParenClose` and
   `ArithmeticEvalClose` only push `)` when they close inner (expression-level)
   parens (i.e., when resulting depth is >= 2, the 2 representing the outer `((`).
   Fixed: `parse-arithmetic-extra-paren.sh`, and likely any other test using
   arithmetic with extra grouping parens.

5. **`$` not escaped in Perl double-quoted string literals in eval handler** —
   The dynamic eval command handler in `redirects.rs` escaped `\`, `"`, `\n`, `\r`
   but not `$` or `@` when building Perl double-quoted string literals from literal
   text parts.  A literal `${` in the generated string (e.g. `"\\${”`) was
   interpreted by Perl as `\` + `${` (variable interpolation start), causing a
   syntax error.  Added `.replace(”$”, “\\$”).replace(”@”, “\\@”)` to both
   escape blocks.  Fixed: `backslash-dollar-brace.sh`.

6. **`# native Perl` comment swallowing closing `}` in eval handler** —
   The `do` block generated for dynamic eval had `# native Perl` before the closing
   `};`, making the `};` part of the comment.  Moved the `};` before the comment.
   This was found while fixing Item 3; together they fixed `backslash-dollar-brace.sh`.

7. **Missing `use IPC::Open3;` in generated Perl** —
   `command_needs_ipc_open3` always returned `false`, saying "No commands need
   IPC::Open3 anymore — all generate native Perl."  But the generator still uses
   `open3` in many code paths (cat, pipeline, command substitution fallbacks).
   Changed to always emit `use IPC::Open3;` (it's harmless when unused).
   Fixed: `backslash-newline-dq.sh` and any other test using command substitutions
   that fall back to open3.

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

## Remaining failures (~172)

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
