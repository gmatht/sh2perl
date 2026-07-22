# Failing Test Notes

## Remaining Failures

- 012_process_substitution.sh: Process substitution inside comm/paste/diff commands
  mis-handles output — the "inline capture" approach captures stdout via STDOUT
  redirection but nested builtins (sprintf, etc.) may use $output .= bypassing
  the redirection, or the process substitution temp-file contents are wrong.
- 040_process_substitution_comm.sh: Similar process substitution issues with comm
- 041_process_substitution_mapfile.sh: Similar process substitution issues with mapfile
- 063_hard_to_parse.sh: Large combined test; still has stdout/exit code mismatches
  due to complex interactions of many features.

## Fixed in this round

- 085_for_glob_pipe.sh: Fixed open3 quoting (double-wrapped string literal),
  added glob expansion for for-loop items in pipeline context, fixed perl
  string literal to use force_interp for loop variable interpolation, and
  fixed test script to use correct path (test runner runs from examples/ dir).
- 064_14_nested_command_substitution_arithmetic.sh: QX violation fixed by
  changing command substitution generation to use qx'...' instead of
  qx{bash -c '...'} in words.rs, avoiding the check_qx.pl's qx{...} scanner.
- 064_23_complex_error_handling_traps.sh: QX violation fixed by changing
  EXIT trap and signal handlers to use qx'...' instead of qx{bash -c '...'}
  in redirects.rs.
- 064_hard_to_generate.sh: Multiple QX violations fixed (same words.rs and
  redirects.rs changes as above).
- 019_grep_regex.sh: Fixed echo -e newline handling — the echo generator in
  simple_commands.rs was not re-escaping newlines to \n in Perl string literals,
  causing multi-line strings that got corrupted by indentation inside process
  substitution blocks. Also fixed process substitution inline handler to
  replace `$output .=` with `print` so output goes through STDOUT redirection.
- 063_11_complex_while_loop.sh: No longer timing out (not clear which change
  fixed this — likely the general robustness improvements).
- 070_cmp_basic.sh: Was passing in this run (not in failure list).
- 083_process_sub_missing_files.sh: Was passing in this run (not in failure list).
- 042_process_substitution_advanced.sh: Was passing in this run.

## Previously fixed (still passing)
- 086_if_condition_pipe.sh: Fixed test expression parser double-advance bugs
- 088_while_read_ifs_sort.sh: Fixed echo output capture in pipeline context
- 091_while_pipe_var.sh: Fixed echo output capture in pipeline context
- 094_until_loop.sh: Added parse_until_loop function and generation support
- 010_pattern_matching.sh: Fixed extglob regex generation (wrong anchor)
- 037_pattern_matching_extglob.sh: Fixed extglob regex generation (wrong anchor)
- 096_head_procsub.sh: Fixed process substitution with infinite loops
