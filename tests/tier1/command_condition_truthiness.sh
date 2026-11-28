#!/bin/bash
# Command conditions must mean what bash means in EVERY position: a command's
# exit status is the condition, and 0 is TRUE. The backend used to render
# `self.expr(cond)` straight into Perl, where the same value is FALSE, so the
# wrong branch ran — silently.
#
# `grep -q` is covered natively (the renderer folds it to an in-Perl regex),
# and the external/`test`/compound forms exercise the shell-status path. The
# `# expected (bash):` line makes the gate prove the DOCUMENTED behaviour
# rather than merely that two runs agree.
# expected (bash): yes|no|neg-yes|elsif-b|while-1|while-2|and-taken|or-taken|arith-7
set -u
cd "$(dirname "$0")" || exit 1
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
printf 'hello\n' >"$work/f.txt"
cd "$work" || exit 1

# if / else: the condition is a command's status, not a boolean value.
if grep -q hello f.txt; then printf 'yes|'; else printf 'WRONG|'; fi
if grep -q zzz f.txt; then printf 'WRONG|'; else printf 'no|'; fi

# `!` must invert the STATUS, not the raw child status.
if ! grep -q zzz f.txt; then printf 'neg-yes|'; else printf 'WRONG|'; fi

# elsif: a false first condition must fall through to the command condition.
if false; then printf 'WRONG|'; elif grep -q hello f.txt; then printf 'elsif-b|'; else printf 'WRONG|'; fi

# while: the loop runs while the command SUCCEEDS.
n=0
while [ "$n" -lt 1 ]; do n=$((n + 1)); done
printf 'while-%s|' "$n"

# An external command (no native lowering) as a loop condition.
i=0
while test "$i" -lt 2; do i=$((i + 1)); done
printf 'while-%s|' "$i"

# Compound positions: `&&` / `||` consume the status too, and a natively
# lowered builtin must not fork (or die) there.
cd /no/such/dir && printf 'WRONG|' || printf 'and-taken|'
false || printf 'or-taken|'

# Arithmetic ternary (the only `?:` shell has) reads the same operands.
printf 'arith-%s\n' "$(( 1 ? 7 : 9 ))"
