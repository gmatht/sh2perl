# Failing Test Notes

## Fixed since last update
- 086_if_condition_pipe.sh: Fixed test expression parser double-advance bugs
  and missing spaces in file test operators
- 088_while_read_ifs_sort.sh: Fixed echo output capture in pipeline context
- 091_while_pipe_var.sh: Fixed echo output capture in pipeline context
- 094_until_loop.sh: Added parse_until_loop function and generation support
- 010_pattern_matching.sh: Fixed extglob regex generation (wrong anchor)
- 037_pattern_matching_extglob.sh: Fixed extglob regex generation (wrong anchor)
- 096_head_procsub.sh: Fixed process substitution with infinite loops
  (used FIFO+fork approach instead of inline capture that hung)

## Fixed
- 063_hard_to_parse.sh: Fixed empty-echo print (suppressed `print q{}`), fixed
  wait() exit code handling (guard against $? = -1 after wait returns -1),
  fixed tr here-string trailing newline
- 064_hard_to_generate.sh: Fixed brace expansion glob in for loops
  (use `sort glob()` with no-match fallback to literal pattern)
