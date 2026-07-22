# Side-Effect Comparison: bash vs Perl

Run: `for f in examples/*.sh; do bash check_side_effects_overlay.sh "$f"; done`
Checker detects: regular files, directories, named pipes, symlinks.

## Results

**162 of 169 tests** have identical side effects between bash and Perl.

**7 tests** have mismatches — all from the same root cause.

## Tests Where Side Effects Match

Every test that passes the Rust test runner also has matching side
effects. This includes tests where the translator cheats via
`system('bash', '-c', ...)` or `open3(...)` — the side effects match
because bash is doing the actual file operations in both cases.

No tests were found where both bash and Perl create files but the
sets differ. This means there are no "false positive" side-effect
differences caused by non-determinism (e.g., temp files with random
names, timestamps, or process IDs). The overlay captures everything
deterministically.

## Tests Where Side Effects Differ

All 7 have the same pattern: bash creates files normally, Perl
creates **nothing** because the translation fails at the first step.

| Test | Bash creates | Perl creates | Root cause |
|------|-------------|-------------|------------|
| 018_grep_params | `temp_file.txt` (file), `test_dir/` (directory), `test_dir/file1.txt`, `test_dir/file2.txt` | nothing | `system()` fails |
| 063_14_complex_redirects | `comparison.txt` | nothing | same |
| 063_20_final_complex_construct | `final_result.txt` | nothing | same |
| 063_hard_to_parse | `comparison.txt`, `final_result.txt` | nothing | same |
| 064_08_heredocs_with_variable_interpolation | `config.txt` | nothing | same |
| 064_15_complex_pipeline_multiple_redirects | `errors.log`, `users.txt` | nothing | same |
| 064_hard_to_generate | `config.txt`, `errors.log`, `users.txt` | nothing | same |

## Root Cause

The translator, when it cannot translate a complex command natively,
falls back to:

```perl
$main_exit_code = system('examples/script.sh') >> 8;
```

This tries to **execute** the `.sh` file. But shell scripts are
**not executable** (`-rw-rw-r--`). The `system()` call fails with
"Permission denied" before the test logic ever runs.

The correct fallback would be:

```perl
$main_exit_code = system('bash', 'examples/script.sh') >> 8;
```

This passes the script as an argument to `bash`, which works regardless
of the execute bit.

## Which Differences Are Real vs Non-Determinism

**All 7 mismatches are real.** The files bash creates are legitimate
test artifacts (grep temp files, diff output, heredoc output, redirect
logs). Perl creates nothing because the entire translation fails
before executing any test logic. There is no non-determinism — every
run produces the same failure.

## What Should Be Fixed

### High Priority: Fix the `system()` Fallback

The `system('script.sh')` → `system('bash', 'script.sh')` fix would
eliminate all 7 mismatches immediately. It's a one-line change in the
code generator. This doesn't fix the underlying translation gap —
Perl would still just run bash — but it would make the side-effect
checker pass and let the actual test logic run so real translation
bugs can be identified.

### Low Priority: Individual Translation Gaps

The 7 tests that trigger the `system()` fallback are complex scripts
that the translator cannot fully handle. Fixing each translation gap
individually would reduce reliance on the `system()` fallback, making
these side-effect differences moot. This is the same work as fixing
the 12 remaining Rust test failures.
