#!/bin/bash
# Pipeline examples (hermetic: runs in its own mktemp scratch, never the
# shared CWD — the old ls/find/file.txt read the harness's cwd)
d=$(mktemp -d)
cd "$d" || exit 1
printf 'apple\napple\n' > file.txt
printf 'hello.txt\n' > note.txt
printf 'function f() { echo hi; }\n' > s.sh
ls | grep "\.txt$" | wc -l
echo
cat file.txt | sort | uniq -c | sort -nr
echo
find . -name "*.sh" | xargs grep -l "function" | tr -d "\\\\/"
echo
cat file.txt | tr 'a' 'b' | grep 'hello'
echo
cat file.txt | sort | grep 'hello'
cd /
rm -rf "$d"
