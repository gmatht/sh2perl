#!/bin/bash

# Grep context and file operation examples — hermetic: fixtures live in a
# mktemp dir, never in the mutable workspace CWD.

# Context lines: after, before, and both
echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET"
echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"

# Recursive search in a scratch dir
d=$(mktemp -d)
cd "$d" || exit 1
echo "pattern in file1" > temp_file1.txt
echo "no pattern in file2" > temp_file2.txt
echo "pattern in file3" > temp_file3.txt

echo "Recursive search results:"
grep -r "pattern" . --include="*.txt"

echo Result 2...
# Print file names with matches
grep -l "pattern" *.txt | sort

echo Result 3...
# Print file names without matches
grep -L "pattern" *.txt

cd /
rm -rf "$d"

matched=$(grep -c ".*" <<< "test")
echo "  grep_exit: $?"
echo "  match_count: $matched"
