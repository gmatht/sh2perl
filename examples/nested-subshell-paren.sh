#!/bin/sh
# Nested subshells with file descriptor redirects
d=$(mktemp -d)
printf 'hi' | gzip > "$d/f1"
printf 'there' | gzip > "$d/f2"
file1="$d/f1"; file2="$d/f2"
result=$(
    (
        gzip -cdfq -- "$file1" 2>/dev/null
        gzip -cdfq -- "$file2" 2>/dev/null
    ) 3>&- 5<&- </dev/null
)
echo "$result"
rm -rf "$d"
