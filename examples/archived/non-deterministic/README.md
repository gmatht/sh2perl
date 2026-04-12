Archived Non-Deterministic Examples
=================================

This directory contains example scripts that were archived because they produced
non-deterministic output when run by the automated determinism test harness.

Why these are archived
- examples/044_find_example.sh: output depends on repository filesystem state (find results vary when temporary files exist).
- examples/052_numeric_computations.sh: contains micro-benchmarks that produce varying timings.
- examples/053_gcd.sh: provides an interactive mode that reads from stdin and behaves differently under automated runs.

If you need to re-enable any of these for testing or examples, consider one of:
- Run the example in a clean/isolated worktree (e.g., a fresh clone or git worktree) so filesystem-dependent commands like find produce stable results.
- Modify the harness to provide deterministic stdin or to normalize timing output before comparisons.
- Move benchmark or interactive sections behind a flag so automated runs can skip them.

How to re-enable
1. Copy the file back to the top-level examples/ directory and commit.
   Example: git checkout HEAD -- examples/044_find_example.sh
2. Or update your test harness to explicitly include examples/archived/non-deterministic/ when collecting scripts.

Guidelines for contributors
- Prefer making examples deterministic where practical (e.g., avoid printing environment-specific paths or timings).
- If an example must remain environment-dependent, keep it in this archive with a short note explaining why.

Maintainer: automated determinism tests
