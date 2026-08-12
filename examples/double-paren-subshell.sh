#!/bin/sh
# (( used for nested subshell (cmd1) | cmd2 ) not arithmetic
d=$(mktemp -d)
printf 'hi' | gzip > "$d/f1"
printf 'there' | gzip > "$d/f2"
result=$(
    (gzip -cdfq -- "$d/f1" 2>/dev/null; gzip -cdfq -- "$d/f2" 2>/dev/null)
)
echo "$result"
rm -rf "$d"
