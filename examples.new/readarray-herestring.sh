#!/usr/bin/env bash
# Demonstrates readarray with here-string containing command substitution
readarray -t files <<<"$(echo test)"
echo "${files[@]}"
