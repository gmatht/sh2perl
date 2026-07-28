#!/bin/sh
# Test: complex arithmetic with parentheses
# Nested arithmetic expressions
a=$(( (1 + 2) * 3 ))
b=$(( ( (1 + 2) * 3 ) / 2 ))
c=$(( a + b ))
echo "$(( a + b + c ))"
