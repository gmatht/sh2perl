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

# ── refused writes and `unset`s in NESTED statements ──
# bash answers a refused ASSIGNMENT by abandoning the body it happened in AND
# every body around it (function, if, loop, then the shell's own statement
# list), so the statements after it never run, the enclosing command reports 1
# and the top-level list CARRIES ON — it does not exit. A refused `unset` fails
# only its own command, so those bodies run to the end. Both paths are here,
# and stdout, stderr and the status are all compared.
LIMIT=31
echo "refuse-top rc=$? after=$LIMIT"

refuse_fn() {
  LIMIT=32
  echo "fn-after-refusal"
  echo "fn-after-rc=$?"
}
refuse_fn
echo "refuse-fn rc=$?"

if [ -n "${LIMIT}x" ]; then
  LIMIT=33
  echo "if-after-refusal"
fi
echo "refuse-if rc=$?"

for i in a b; do
  LIMIT=34
  echo "for-after-refusal"
done
echo "refuse-for rc=$?"

while read -r w; do
  LIMIT=35
  echo "while-after-refusal"
done <<EOF
line1
line2
EOF
echo "refuse-while rc=$?"

{ LIMIT=36; echo "block-after-refusal"; }
echo "refuse-block rc=$?"

( LIMIT=37; echo "sub-after-refusal" )
echo "refuse-sub rc=$?"

unset_refuse_fn() {
  unset LIMIT
  echo "unset-fn-after rc=$?"
}
unset_refuse_fn
echo "unset-fn rc=$?"

if [ -n "${LIMIT}x" ]; then
  unset LIMIT
  echo "unset-if-after rc=$?"
fi
echo "unset-if rc=$?"

for i in a b; do
  unset LIMIT
  echo "unset-for-after rc=$?"
done
echo "unset-for rc=$?"

while read -r w; do
  unset LIMIT
  echo "unset-while-after rc=$?"
done <<EOF
line1
line2
EOF
echo "unset-while rc=$?"

( unset LIMIT; echo "unset-sub-after rc=$?" )
echo "unset-sub rc=$?"
echo "limit=$LIMIT"
