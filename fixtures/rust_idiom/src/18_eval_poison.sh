# The store half of the promotion boundary, same shape as 11_promote's `n`:
# a top-level counter written and read lexically. `eval` can reach any name
# at run time, so the whole program keeps its store — a future loosening of
# the slice must fail this fixture instead of silently changing behaviour.
n=0
n=$((n + 1))
eval 'n=$((n + 10))'
echo "$n"
