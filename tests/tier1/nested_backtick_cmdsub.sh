#!/bin/bash
# Tier 1, class 3: a nested `$()` inside a BACKTICK capture.
#
# The inner substitution must be computed natively (or left to the outer qx);
# escaping it into the backtick's shell text turned `$(pwd)` into a literal
# `$(pwd)` word, so the child ran `basename` with NO argument and printed
# "basename: missing operand" instead of the directory name.
mkdir -p nested/sub
printf 'file body\n' > nested/sub/data.txt

x=`pwd`
echo "x=$(basename "$x")"

y=`basename $(pwd)`
echo "y=$y"

z=`cd nested && basename $(pwd)`
echo "z=$z"

# Nested inside a $() that is itself inside a backtick.
w=`echo $(basename $(pwd))`
echo "w=$w"

# The same nesting in a plain $() capture (the value must be identical).
v=$(basename $(pwd))
echo "v=$v"
