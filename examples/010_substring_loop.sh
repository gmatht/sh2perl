#!/usr/bin/env bash
# Regression test: substring extraction inside a while loop.
# The loop counter must update every iteration so ${s:$i:1} reads each
# character in turn — a LICM / dead-store-elim bug can hoist the
# setVar("ch", slice(...)) outside the loop, making every iteration
# read index 0 (the first character) instead of the current index.
#
# expected (bash): HELLOWORLD
#   estree bug:    HHHHHHHHHH   (all first chars — slice hoisted)
s="HELLOWORLD"
i=0
len=${#s}
while [ "$i" -lt "$len" ]; do
  ch=${s:$i:1}
  printf "%s" "$ch"
  i=$((i + 1))
done
echo ""
