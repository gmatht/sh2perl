#!/usr/bin/env bash
# The ONE variable store, in the states that matter: write-then-read, export
# into a child's environment, readonly rejection, subshell isolation, and a
# stored value read inside a pipeline stage. Every name here is deliberately
# unpromotable (`export`/`readonly`/`unset` poison it, `n` is written in a
# function and read in main), so the whole script runs out of `__SH_VARS`.
# stderr-exact: bash's readonly diagnostics (`<script>: line N: …`) and the
# `$?` after them must match byte for byte, not just stdout and status.
export GREETING=hello
GREETING="$GREETING world"
echo "greeting=$GREETING"
bash -c 'echo "child=$GREETING"'

readonly LIMIT=3
LIMIT=9
echo "limit=$LIMIT rc=$?"
unset LIMIT
echo "unset-ro=$LIMIT rc=$?"

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

# ── nested refusals name the OFFENDING statement's line ──
# bash aborts the enclosing compound BODY when an assignment is refused (the
# failing statement is the last one that runs in that body), while a refused
# `unset` fails only that command and the body continues — so each shape below
# carries exactly one refusal and `$?` is observed after the compound, which is
# where both shells still agree.
in_fn() {
  LIMIT=21
}
in_fn
echo "refuse-fn rc=$?"
if [ -n "${LIMIT}x" ]; then
  LIMIT=22
fi
echo "refuse-if rc=$?"
for i in a; do
  LIMIT=23
done
echo "refuse-for rc=$?"
while read -r w; do
  LIMIT=24
done <<EOF
once
EOF
echo "refuse-while rc=$?"
( LIMIT=25 )
echo "refuse-sub rc=$?"

unset_fn() {
  unset LIMIT
}
unset_fn
echo "unset-fn rc=$?"
if [ -n "${LIMIT}x" ]; then
  unset LIMIT
fi
echo "unset-if rc=$?"
for i in a b; do
  unset LIMIT
done
echo "unset-for rc=$?"
while read -r w; do
  unset LIMIT
done <<EOF
once
EOF
echo "unset-while rc=$?"
( unset LIMIT )
echo "unset-sub rc=$?"
echo "limit=$LIMIT"
