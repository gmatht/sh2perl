#!/bin/bash
# Pipeline failure demo: basic pipe returns empty in pure Perl mode
# (hermetic: runs in its own mktemp scratch, never the shared /tmp)
d=$(mktemp -d)
cd "$d" || exit 1
printf 'alpha\n' > alpha
printf 'beta\n' > beta
printf 'gamma\n' > gamma
echo "File list:"
ls -1 | head -3
echo "---"
echo "Count:"
ls | wc -l
echo "done"
cd /
rm -rf "$d"
