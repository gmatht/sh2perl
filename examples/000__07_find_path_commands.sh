#!/bin/bash

# find command with backticks — hermetic: search a mktemp tree, not the CWD
# (the expected output must not be a listing of the mutable workspace)
#PERL_MUST_NOT_CONTAIN `find
d=$(mktemp -d)
mkdir -p "$d/sub"
touch "$d/a.sh" "$d/b.sh" "$d/sub/c.sh" "$d/data.txt"
found_files=$(cd "$d" && find . -name "*.sh" -type f | sort)
echo "Found shell scripts:"
echo "$found_files"
rm -rf "$d"
