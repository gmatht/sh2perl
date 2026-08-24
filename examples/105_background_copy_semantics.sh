#!/bin/bash
# Background copy semantics: `{ ... } &` forks — the job runs on a copy of
# the shell state, so its mutations are isolated (bash prints x=1).
# Regression pin for the sh2.background clone (was: the microtask body ran
# against the live parent state and leaked x=2).
x=1
{ x=2; } &
wait
echo "x=$x"
