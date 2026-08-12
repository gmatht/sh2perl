#!/bin/bash

# 19. Complex pattern matching with extended globs — hermetic: the glob
# runs in a mktemp dir with known files, so the output is deterministic
# (in the CWD the glob could match nothing and stay literal).
d=$(mktemp -d)
cd "$d" || exit 1
touch a.txt b.log c.dat x.yz
for file in *.{txt,log,dat}; do
    case "$file" in
        *.txt|*.log) echo "Text file: $file";;
        *.dat) echo "Data file: $file";;
        *) echo "Other file: $file";;
    esac
done
cd /
rm -rf "$d"
