# Failing Test Notes

## All tests passing (512/512) 🎉

All tests pass with no failures or timeouts.

## No remaining check_qx.pl violations

All qx{}/system() violations have been resolved.

## Fixed issues

1. **parse_command_redirects double call + newline consumption**: The `parse_command_redirects`
   function was called twice (once from `parse_pipeline` and once from `parse_command`).
   The first call to `skip_whitespace_and_comments()` (and similar calls in 
   `parse_pipeline_from_command` and `parse_command`) consumed newlines that separated
   commands, causing the second call's arg-collection loop to steal tokens from the
   next command as additional arguments. Fixed by using `skip_inline_whitespace_and_comments`
   instead (preserving newlines) and adding a standalone-assignment check in the
   arg-collection loop.

2. **Standalone assignment operator lost**: `parse_standalone_assignment` read the
   assignment operator (`+=`, `-=`, etc.) but always constructed `Command::Assignment`
   with `AssignmentOperator::Assign`, losing the operator information. Fixed by
   tracking the operator in a separate `BTreeMap` and using it when constructing
   the Assignment command.

3. **Heredoc pipeline timeout (063_05_heredoc_with_complex_content.sh)**: Two issues:
   - The IR-based clean path in `generate_buffered_pipeline` used qx{...} for pipeline
     capture, but `generate_bash_command_string` did not serialize heredoc redirects,
     causing the heredoc content to be lost and `command cat` to hang waiting for stdin.
     Fixed by detecting heredocs in the pipeline and skipping the clean path.
   - The `do { ... }` block emitted by the scaffolding path lacked a trailing semicolon,
     causing a Perl syntax error (`do BLOCK` without semicolon is ambiguous).
     Fixed by changing `"}\n"` to `"};\n"`.

4. **Temp file race condition in parallel tests**: The hard-coded temp file name
   `/tmp/__tmp_test_output.pl` caused race conditions when multiple test workers
   ran in parallel, overwriting each other's temporary Perl files. Fixed by using
   unique temp file names based on the test filename.

5. **$PROGRAM_NAME assignment without use English**: The generator emitted
   `$PROGRAM_NAME = '...'` when `set_original_script_name` was called, but didn't
   ensure `use English` was imported, causing "Global symbol requires explicit
   package name" errors. Fixed by using `$0` directly instead of `$PROGRAM_NAME`.

6. **Missing $main_exit_code declaration for pipeline scaffolding**: The pipeline
   scaffolding path references `$main_exit_code` but `needs_exit_code_tracking`
   only returned true for pipelines with >2 commands, causing compilation errors.
   Fixed by making `needs_exit_code_tracking` return true for any pipeline.

7. **wc format string trailing space**: The wc command generator used
   `fmt_parts.join(" ")` to join format parts, which added a space between `%d`
   and `\n` even for single-column output, producing output like `"1 "` instead
   of `"1"`. Fixed by joining without spaces for single-column output.

8. **Subshell in pipeline line-splitting corrupts multi-line literals**: When a
   subshell command (containing heredocs) was the first command in a pipeline,
   the scaffolding path split the generated Perl code into lines and re-indented
   each line, corrupting multi-line string literals like `q{...}`. Fixed by
   treating Subshell commands like Redirect commands (using the full generator
   output without line-splitting).
