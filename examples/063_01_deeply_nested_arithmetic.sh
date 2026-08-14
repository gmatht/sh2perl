#!/bin/bash

# 1. Deeply nested arithmetic expressions with mixed operators
a=10 b=3 c=8 d=2 e=7 f=2 g=2 h=3 i=4 j=1 k=6 l=3 m=5 n=2
result=$(( (a + b) * (c - d) / (e % f) + (g ** h) - (i << j) | (k & l) ^ (m | n) ))
echo "Deeply nested arithmetic result: $result"
