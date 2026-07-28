#!/bin/sh
# Backslash-newline inside a double-quoted string with command substitution
tempfile="$(mktemp --tmpdir prefix-XXXXXXXX 2>/dev/null \
    || mktemp -t prefix-XXXXXXXX)"
echo "$tempfile"
