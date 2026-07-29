# Failing Test Notes

## Current status: 173/517 passed, 344 failing

After the `--test-file` error-propagation fix, previously silent failures are now
reported. The following systematic issues were fixed:

## Fixed issues (this session)

1. **`say` used without `use feature 'say'`**: The IR statement emitter used `say`
   for `Output { newline: true }` but the generator preamble did not include
   `use feature 'say'`. Fixed by emitting `print` with embedded `\n` instead of
   `say` in `emit_stmt()` (ir.rs).

2. **`cmd_str_to_open_perl` returned empty string**: The function was supposed to
   convert a shell command string into Perl code using `open()` instead of `qx{...}`,
   but returned `"q{}"` (empty string constant). Fixed to produce a proper
   `do { open(my $fh, '-|', 'bash', '-c', q{...}) or die ... }` block (ir.rs).

3. **Missing `$__set_e` declaration**: The `set -e` handler (for `set -e`/`set -euo`)
   emitted `$__set_e = 1;` but the variable declaration `my $__set_e = 0;` was only
   emitted before processing commands, when `set_e_active` was still false. Fixed by
   adding a pre-scan for `set -e` that sets `set_e_active` before the declaration
   block (mod.rs).

4. **Missing `$CHILD_ERROR` declaration**: The `our $CHILD_ERROR;` declaration was
   only emitted when `needs_ipc_open3` or `needs_exit_code_tracking` returned true.
   Scripts with backtick command substitutions (which reference `$CHILD_ERROR`)
   but without pipelines/operators missed the declaration. Fixed by adding
   `has_command_substitution` check (mod.rs).

5. **`croak` used without `use Carp`**: Many command generators (cp, mv, rm, mkdir,
   touch, cat, grep, etc.) emit `croak()` calls but `use Carp` was only imported
   for a few commands. Fixed by always importing `use Carp` (mod.rs).

6. **`$OS_ERROR`/`$ERRNO` used without `use English`**: The same generators use
   English variable names but `use English` was only conditionally imported.
   Fixed by always importing `use English` (mod.rs).

7. **`$0` -> `$PROGRAM_NAME` conversion required `use English`**: The generator
   converted shell `$0` to Perl `$PROGRAM_NAME` which requires the English module.
   Changed to use `$0` directly (words.rs, utils.rs).

8. **Pre-analysis detection functions missed `Redirect`/`Not` wrappers**:  
   `command_uses_ls`, `command_uses_locale`, and `command_uses_english` did not
   recurse into `Command::Redirect` or `Command::Not`, causing variables like
   `$ls_success` to go undeclared when commands were wrapped in redirects.
   Fixed by adding the missing match arms.

## Remaining failures (~344)

The remaining failures fall into several categories:

1. **Runtime stderr warnings** (e.g., uninitialized values, filehandle issues):
   Generated Perl code compiles but produces warnings on stderr that bash does not.
   Many of these are due to incomplete variable initialization or filehandle
   management in the command generators.

2. **stdout formatting mismatches**: The translated output differs from bash
   output (e.g., extra blank lines, missing content, wrong quoting). These are
   specific to individual command generators and string interpolation code.

3. **Test expression issues** (`[[ ... ]]`): Command substitution inside test
   expressions (like `[[ $(uname -r) == 5.4.* ]]`) is not properly translated,
   producing literal `$(uname` in the Perl output which is a syntax error.

4. **Edge cases in parameter expansion, heredocs, arrays**: Various edge cases
   in complex shell constructs that the generator does not handle correctly.

5. **Pipeline output discrepancies**: Some pipelines produce extra blank lines
   or missing output compared to bash.

Each of these requires targeted fixes in specific generator modules.
