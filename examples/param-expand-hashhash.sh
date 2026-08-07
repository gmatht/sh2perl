#!/bin/sh
# Test that ${p##*/} (parameter expansion with ## pattern removal)
# is parsed correctly.  The ## creates a Comment token in the lexer
# that must be properly handled inside ${...}.
p=/usr/local/bin/example
cmd=${p##*/}
exec echo "$cmd"
