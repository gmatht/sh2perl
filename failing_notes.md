# Failing Test Notes

## All tests passing (169/169) 🎉

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
