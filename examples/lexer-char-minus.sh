#!/bin/sh
# Lexer error: unexpected character '-'
# This tests patterns that might confuse the lexer with '-' characters
set -e
export LANG=C
test "a" = "a"
printf "parsed OK\\n"

echo "done: $?"
