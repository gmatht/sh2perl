#!/bin/sh
# The -ef test operator was not handled in parse_word
# (hermetic: two hardlinked files in a scratch dir, so the comparison is real)
d=$(mktemp -d)
touch "$d/a" "$d/b"
ln "$d/a" "$d/a2"
test "$d/a" -ef "$d/a2" && echo "same file"
test "$d/a" -ef "$d/b" || echo "different files"
rm -rf "$d"
