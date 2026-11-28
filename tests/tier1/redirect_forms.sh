#!/bin/bash
# Redirect forms the emitter lowers itself, in the positions a script uses
# them: a statement redirect on a native builtin, append, input, a group
# redirect, `2>/dev/null`, and the `$?` a successful redirect leaves behind.
# Two shapes are deliberately NOT here because the emitter does not lower them
# yet (both are recorded in tests/bash-parity-known-gaps.txt, and a fixture
# that pins a wrong answer would only launder it): `exec 3>file` / `>&3` (an
# arbitrary fd) and `1>&2` on a native `printf`, which currently loses the
# bytes instead of duping them. A FAILED redirect's diagnostic is excluded for
# the documented `script: line N: ` prefix reason.
#
# A stderr DUP on a natively lowered builtin is pinned separately, in
# tests/tier1/dup_stderr_to_stdout.sh: that is where `2>&1` used to create a
# file literally named `&1` and swallow the diagnostic.
echo alpha > f1
echo beta >> f1
cat f1
echo out
echo err >&2
{ echo both; echo bad >&2; } > f2 2>&1
cat f2
wc -l < f1
echo quiet 2>/dev/null
echo y > /dev/null; echo "after=$?"
printf 'z\n' > f3; cat f3
rm -f f1 f2 f3 f4
echo done

# expected (bash):
# alpha
# beta
# out
# both
# bad
# 2
# quiet
# after=0
# z
# done
