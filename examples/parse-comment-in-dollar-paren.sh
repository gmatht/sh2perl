#!/bin/bash
# Comment (#) inside ${...} parameter expansion inside $(...)
# This tests that the parser handles # inside ${...} correctly
# even when the ${...} is inside a $(...) command substitution.
TempDir=$(mktemp -d /tmp/${0##*/}.XXXXXX || exit 2)
echo "$TempDir"
