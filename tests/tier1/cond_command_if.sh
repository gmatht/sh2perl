#!/bin/bash
# Tier 1, class 1: a COMMAND condition is bash's status test.
#
# `if cmd` is true when cmd exits 0. Perl's `system()` returns 0 on success,
# so a bare `if (system(...))` inverts every command condition — the emitter
# must boolify the status (status == 0) everywhere: if, elif, while, until,
# negated `!`, and the `cond && a || b` ternary shape.
printf 'needle in a haystack\n' > haystack.txt

if grep -q needle haystack.txt; then echo if-hit; else echo if-miss; fi
if grep -q absent haystack.txt; then echo if-hit; else echo if-miss; fi

if false; then
    echo elif-never
elif grep -q needle haystack.txt; then
    echo elif-hit
else
    echo elif-miss
fi

if false; then
    echo elif-never
elif grep -q absent haystack.txt; then
    echo elif-hit
else
    echo elif-miss
fi

if ! grep -q absent haystack.txt; then echo not-hit; else echo not-miss; fi

# while with a command condition: runs exactly while grep succeeds.
n=0
while grep -q needle haystack.txt; do
    n=$((n + 1))
    [ "$n" -ge 2 ] && break
done
echo "while-iterations=$n"

# until with a command condition: runs while grep fails.
m=0
until grep -q needle haystack.txt; do
    m=$((m + 1))
    [ "$m" -ge 3 ] && break
done
echo "until-iterations=$m"

# Ternary shape (`cmd && success || failure`) on a command.
grep -q needle haystack.txt && echo ternary-hit || echo ternary-miss
grep -q absent haystack.txt && echo ternary-hit || echo ternary-miss
