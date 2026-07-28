#!/bin/sh
# Mixed quoting patterns
x="$HOME/file"
y='literal string'
z="$x with $y and ${x}ext"
echo "$z"
