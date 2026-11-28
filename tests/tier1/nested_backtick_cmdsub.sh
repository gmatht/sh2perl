#!/bin/bash
# Tier 1, class 3: a nested `$()` inside a BACKTICK capture, and backticks as
# one PART of a word.
#
# The inner substitution must be computed natively (or left to the outer qx);
# escaping it into the backtick's shell text turned `$(pwd)` into a literal
# `$(pwd)` word, so the child ran `basename` with NO argument and printed
# "basename: missing operand" instead of the directory name.
#
# The second half is the word-shape rule: bash's backticks are a word part,
# not a word. Returning after the first pair dropped the rest of the word
# (`` `pwd`-`pwd` `` printed only the first half) and left the token stream
# misaligned, which swallowed the statements that followed.
# expected (bash):
# x=work
# y=work
# z=nested
# w=work
# v=work
# q=work
# K=work-work
# M=prehipost
# N=hibye
# O=workwork
# S=work/work
# T=work work work
# W=work
# U=work:work
# R=ok
# rc-after-false=1
# P=work

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

# Quoted inner substitution inside the backtick body.
q=`basename "$(pwd)"`
echo "q=$q"

# Two backtick substitutions in ONE word, and text around them.
d=`basename $(pwd)`
K=`echo $d`-`echo $d`
echo "K=$K"

# Backtick substitution in the MIDDLE of a word.
M=pre`echo hi`post
echo "M=$M"

# Two adjacent backtick substitutions (bash concatenates them).
N=`echo hi``echo bye`
echo "N=$N"

O=`basename $(pwd)``basename $(pwd)`
echo "O=$O"

# A backtick pair as one half of a path word.
S=`echo $d`/`echo $d`
echo "S=$S"

# Several inner substitutions in one backtick body.
T=`echo $(basename $(pwd)) $(basename $(pwd)) $(basename $(pwd))`
echo "T=$T"

# Backticks inside a $() body.
W=$(basename `pwd`)
echo "W=$W"

U=`echo $(basename $(pwd)):$(basename $(pwd))`
echo "U=$U"

R=`echo ok`
echo "R=$R"

# The status after such an assignment: the capture itself cannot succeed, so
# a following `false` must be what `$?` sees.
P=`basename $(pwd)`; false; echo "rc-after-false=$?"
echo "P=$P"
