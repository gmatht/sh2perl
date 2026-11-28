#!/bin/bash
# `(( expr ))` as a condition must read the variables the statement list just
# wrote. It lowers to `exec("let", …)` with the arithmetic TEXT in the arg
# list, so a store whose only read lives inside that text must survive both
# store eliminators and the declaration census: `x=3; if (( x > 1 ))` compared
# an undef and took the else branch while bash took the then branch, and
# `x=3; if (( x > 1 )); …; x=9` dropped the FIRST store as redundant.
#
# PERL_MUST_NOT_CONTAIN: system(
# PERL_MUST_NOT_CONTAIN: Use of uninitialized value

x=3
if (( x > 1 )); then echo one; else echo notone; fi
x=9
if (( x > 5 )); then echo two; else echo notwo; fi
x=1
if (( x > 5 )); then echo three; else echo nothree; fi
i=0
while (( i < 2 )); do i=$((i+1)); done
echo "i=$i"
y=5
if (( y % 2 )); then echo odd; else echo even; fi
z=7
if (( z - 7 )); then echo nz; else echo zero; fi
w=4
if (( w == 4 )); then echo eq; fi
let q=2
if (( q * 2 == 4 )); then echo letq; fi
echo done

# expected (bash):
# one
# two
# nothree
# i=2
# odd
# zero
# eq
# letq
# done
