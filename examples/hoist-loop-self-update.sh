#!/usr/bin/env bash
# A loop-carried self-update is not loop-invariant. Hoisting
# i=$((i + 1)) out of this loop would make the condition stay true.
i=0
while [ "$i" -lt 3 ]; do
  echo "i=$i"
  i=$((i + 1))
done
