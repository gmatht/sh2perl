#!/bin/bash
# ${var#pattern} inside $(...) - # starts a line comment in lexer
# but it's actually a parameter expansion operator.
p=/home/user/script
result=$(echo ${p#/})
echo "$result"
