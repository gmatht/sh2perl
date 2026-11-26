#!/usr/bin/env bash
# The ONE variable store, in the states that matter: write-then-read, export
# into a child's environment, readonly rejection, subshell isolation, and a
# stored value read inside a pipeline stage. Every name here is deliberately
# unpromotable (`export`/`readonly`/`unset` poison it, `n` is written in a
# function and read in main), so the whole script runs out of `__SH_VARS`.
export GREETING=hello
GREETING="$GREETING world"
echo "greeting=$GREETING"
bash -c 'echo "child=$GREETING"'

readonly LIMIT=3
LIMIT=9
echo "limit=$LIMIT rc=$?"

unset GREETING
echo "unset=[$GREETING]"

n=5
grow() {
  n=7
  echo "infn=$n"
}
grow
echo "afterfn=$n"

( n=11; echo "sub=$n" )
echo "aftersub=$n"

echo alpha | while read -r w; do echo "pipe=$w n=$n"; done
echo "after=$n"
