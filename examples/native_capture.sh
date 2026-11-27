#!/bin/bash
# A command substitution whose body GENUINELY cannot constant-fold: the value
# depends on a variable and on two `echo` builtins, so the renderer must emit
# the native capture (`do { … local *STDOUT … }`) rather than a folded
# literal. `000__02`'s captures all fold, so before this example the
# native-capture branch was exercised by tests only.
greeting="hi"
x=$(echo "$greeting"; echo second)
echo "$x"
printf '%s\n' "$x"
